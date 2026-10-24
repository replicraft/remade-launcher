//! The local protocol between `remade`, the desktop app, and the updater.
//!
//! It runs over a Unix domain socket inside a folder only the current user
//! can enter, and the socket checks the peer's user ID against its own on
//! every connection. Requests are one JSON line, at most 4 KiB, and each
//! carries a nonce that is never accepted twice, so a captured request
//! cannot be replayed. Nothing listens on a network port.

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// The largest request line, in bytes.
pub const MAX_LINE: usize = 4096;

/// The version of this protocol.
pub const PROTOCOL_VERSION: u32 = 1;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "request", rename_all = "snake_case")]
pub enum Request {
	/// Read-only: how the updater is doing.
	Status,
	/// Ask for an update pass now. The mode and every other rule still apply.
	CheckNow,
	/// Ask the updater to exit.
	Stop,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Envelope {
	pub v: u32,
	pub nonce: String,
	#[serde(flatten)]
	pub request: Request,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "response", rename_all = "snake_case")]
pub enum Response {
	Status(DaemonInfo),
	Accepted { detail: String },
	Refused { reason: String },
}

/// The updater's own account of itself.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DaemonInfo {
	pub pid: u32,
	pub started_at: i64,
	pub update_mode: String,
	pub last_attempted: Option<i64>,
	pub last_completed: Option<i64>,
	pub next_due: Option<i64>,
	pub last_outcome: Option<String>,
	pub cycle_running: bool,
	/// Requests waiting for their turn. Zero or one: a second request while
	/// one is queued is folded into it.
	pub queue_length: usize,
}

#[derive(Debug, Error)]
pub enum IpcError {
	#[error("the background updater is not running")]
	NotRunning,
	#[error("this system has no local socket for the updater yet")]
	Unsupported,
	#[error("the updater did not answer in time")]
	TimedOut,
	#[error("the updater sent something this version cannot read")]
	BadReply,
	#[error("the socket could not be used: {0}")]
	Io(#[from] std::io::Error),
}

#[cfg(unix)]
mod unix {
	use std::collections::VecDeque;
	use std::future::Future;
	use std::os::unix::fs::{MetadataExt, PermissionsExt};
	use std::path::Path;
	use std::pin::Pin;
	use std::sync::{Arc, Mutex};
	use std::time::Duration;

	use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
	use tokio::net::{UnixListener, UnixStream};

	use super::{Envelope, IpcError, MAX_LINE, PROTOCOL_VERSION, Request, Response};

	const CLIENT_TIMEOUT: Duration = Duration::from_secs(5);
	const READ_TIMEOUT: Duration = Duration::from_secs(5);
	/// How many recent nonces are remembered.
	const NONCE_MEMORY: usize = 256;

	pub trait Handler: Send + Sync {
		fn handle(&self, request: Request) -> Pin<Box<dyn Future<Output = Response> + Send + '_>>;
	}

	/// Binds the updater's socket, readable only by this user.
	///
	/// A stale socket file from a crashed updater is removed. A live one is
	/// not: if something answers on it, binding fails.
	pub async fn bind(socket: &Path) -> Result<UnixListener, IpcError> {
		let folder = socket.parent().ok_or(IpcError::Unsupported)?;

		std::fs::create_dir_all(folder)?;
		std::fs::set_permissions(folder, std::fs::Permissions::from_mode(0o700))?;

		if socket.exists() {
			if UnixStream::connect(socket).await.is_ok() {
				return Err(IpcError::Io(std::io::Error::new(
					std::io::ErrorKind::AddrInUse,
					"another updater is answering",
				)));
			}

			std::fs::remove_file(socket)?;
		}

		let listener = UnixListener::bind(socket)?;

		std::fs::set_permissions(socket, std::fs::Permissions::from_mode(0o600))?;

		Ok(listener)
	}

	/// Answers requests until `stop` resolves. Every connection is checked
	/// against this process's own user ID before a byte is read.
	pub async fn serve(
		listener: UnixListener,
		socket: &Path,
		handler: Arc<dyn Handler>,
		stop: tokio::sync::watch::Receiver<bool>,
	) {
		let own_uid = std::fs::metadata(socket).map(|meta| meta.uid()).ok();
		let seen: Arc<Mutex<VecDeque<String>>> = Arc::default();
		let mut stop = stop;

		loop {
			let accepted = tokio::select! {
				accepted = listener.accept() => accepted,
				_ = stop.changed() => break,
			};

			let Ok((stream, _)) = accepted else { continue };

			// Only the user who owns the socket may talk to it, whatever the
			// file permissions say.
			let peer_ok = stream.peer_cred().ok().map(|credentials| credentials.uid()) == own_uid
				&& own_uid.is_some();

			if !peer_ok {
				tracing::warn!("refused a connection from another user");
				continue;
			}

			let (handler, seen) = (Arc::clone(&handler), Arc::clone(&seen));

			tokio::spawn(async move {
				let _ = tokio::time::timeout(READ_TIMEOUT, answer(stream, handler, seen)).await;
			});
		}
	}

	async fn answer(
		stream: UnixStream,
		handler: Arc<dyn Handler>,
		seen: Arc<Mutex<VecDeque<String>>>,
	) {
		let (read, mut write) = stream.into_split();
		let mut line = Vec::new();

		// One byte over the limit tells an oversized request from one that
		// fits exactly.
		let mut limited = BufReader::new(read).take(MAX_LINE as u64 + 1);

		if limited.read_until(b'\n', &mut line).await.is_err() {
			return;
		}

		let reply = if line.len() > MAX_LINE {
			Response::Refused {
				reason: "that request is too large".into(),
			}
		} else {
			match serde_json::from_slice::<Envelope>(&line) {
				Err(_) => Response::Refused {
					reason: "that is not a request this updater understands".into(),
				},
				Ok(envelope) if envelope.v != PROTOCOL_VERSION => Response::Refused {
					reason: "unsupported protocol version".into(),
				},
				Ok(envelope) if envelope.nonce.is_empty() || envelope.nonce.len() > 64 => {
					Response::Refused {
						reason: "a request needs a nonce".into(),
					}
				}
				Ok(envelope) => {
					if remember(&seen, &envelope.nonce) {
						handler.handle(envelope.request).await
					} else {
						Response::Refused {
							reason: "that request was already used".into(),
						}
					}
				}
			}
		};

		if let Ok(mut text) = serde_json::to_string(&reply) {
			text.push('\n');
			let _ = write.write_all(text.as_bytes()).await;
		}
	}

	/// Records `nonce`. False if it was seen before.
	fn remember(seen: &Mutex<VecDeque<String>>, nonce: &str) -> bool {
		let Ok(mut seen) = seen.lock() else {
			return false;
		};

		if seen.iter().any(|existing| existing == nonce) {
			return false;
		}

		if seen.len() >= NONCE_MEMORY {
			seen.pop_front();
		}

		seen.push_back(nonce.to_owned());

		true
	}

	/// Sends one request to the updater and returns its reply.
	pub async fn send(socket: &Path, request: Request) -> Result<Response, IpcError> {
		let work = async {
			let stream = UnixStream::connect(socket)
				.await
				.map_err(|error| match error.kind() {
					std::io::ErrorKind::NotFound | std::io::ErrorKind::ConnectionRefused => {
						IpcError::NotRunning
					}
					_ => IpcError::Io(error),
				})?;

			let (read, mut write) = stream.into_split();
			let envelope = Envelope {
				v: PROTOCOL_VERSION,
				nonce: uuid::Uuid::now_v7().to_string(),
				request,
			};
			let mut text = serde_json::to_string(&envelope).map_err(|_| IpcError::BadReply)?;

			text.push('\n');
			write.write_all(text.as_bytes()).await?;

			let mut reply = String::new();

			BufReader::new(read)
				.take(64 * 1024)
				.read_line(&mut reply)
				.await?;

			serde_json::from_str(&reply).map_err(|_| IpcError::BadReply)
		};

		tokio::time::timeout(CLIENT_TIMEOUT, work)
			.await
			.map_err(|_| IpcError::TimedOut)?
	}

	#[cfg(test)]
	mod tests {
		use super::*;
		use crate::ipc::DaemonInfo;

		struct Echo;

		impl Handler for Echo {
			fn handle(
				&self,
				request: Request,
			) -> Pin<Box<dyn Future<Output = Response> + Send + '_>> {
				Box::pin(async move {
					match request {
						Request::Status => Response::Status(DaemonInfo {
							pid: 7,
							update_mode: "daemon".into(),
							..DaemonInfo::default()
						}),
						Request::CheckNow => Response::Accepted {
							detail: "queued".into(),
						},
						Request::Stop => Response::Accepted {
							detail: "stopping".into(),
						},
					}
				})
			}
		}

		async fn running(
			directory: &Path,
		) -> (std::path::PathBuf, tokio::sync::watch::Sender<bool>) {
			let socket = directory.join("background/updater.sock");
			let listener = bind(&socket).await.unwrap();
			let (stop_tx, stop_rx) = tokio::sync::watch::channel(false);
			let path = socket.clone();

			tokio::spawn(async move { serve(listener, &path, Arc::new(Echo), stop_rx).await });

			(socket, stop_tx)
		}

		#[tokio::test]
		async fn a_request_round_trips() {
			let directory = tempfile::tempdir().unwrap();
			let (socket, _stop) = running(directory.path()).await;

			let status = send(&socket, Request::Status).await.unwrap();
			let check = send(&socket, Request::CheckNow).await.unwrap();

			assert!(
				matches!(status, Response::Status(info) if info.pid == 7 && info.update_mode == "daemon")
			);
			assert_eq!(
				check,
				Response::Accepted {
					detail: "queued".into()
				}
			);
		}

		#[tokio::test]
		async fn only_the_owner_can_reach_the_socket() {
			let directory = tempfile::tempdir().unwrap();
			let (socket, _stop) = running(directory.path()).await;

			let socket_mode = std::fs::metadata(&socket).unwrap().permissions().mode() & 0o777;
			let folder_mode = std::fs::metadata(socket.parent().unwrap())
				.unwrap()
				.permissions()
				.mode() & 0o777;

			assert_eq!(socket_mode, 0o600);
			assert_eq!(folder_mode, 0o700);
		}

		async fn raw(socket: &Path, bytes: &[u8]) -> String {
			let mut stream = UnixStream::connect(socket).await.unwrap();

			stream.write_all(bytes).await.unwrap();

			let mut reply = String::new();

			BufReader::new(stream).read_line(&mut reply).await.unwrap();

			reply
		}

		#[tokio::test]
		async fn a_replayed_request_is_refused() {
			let directory = tempfile::tempdir().unwrap();
			let (socket, _stop) = running(directory.path()).await;
			let line = br#"{"v":1,"nonce":"abc","request":"check_now"}
"#;

			assert!(raw(&socket, line).await.contains("accepted"));
			assert!(raw(&socket, line).await.contains("already used"));
		}

		#[tokio::test]
		async fn malformed_oversized_and_wrong_version_requests_are_refused() {
			let directory = tempfile::tempdir().unwrap();
			let (socket, _stop) = running(directory.path()).await;

			assert!(raw(&socket, b"not json\n").await.contains("refused"));
			assert!(
				raw(
					&socket,
					br#"{"v":2,"nonce":"n1","request":"status"}
"#
				)
				.await
				.contains("unsupported protocol")
			);
			assert!(
				raw(
					&socket,
					br#"{"v":1,"nonce":"","request":"status"}
"#
				)
				.await
				.contains("nonce")
			);
			assert!(
				raw(
					&socket,
					br#"{"v":1,"nonce":"n2","request":"launch_missiles"}
"#
				)
				.await
				.contains("refused")
			);

			let huge = format!(
				"{{\"v\":1,\"nonce\":\"{}\",\"request\":\"status\"}}\n",
				"x".repeat(MAX_LINE)
			);
			assert!(raw(&socket, huge.as_bytes()).await.contains("too large"));
		}

		#[tokio::test]
		async fn nothing_listening_is_reported_as_not_running() {
			let directory = tempfile::tempdir().unwrap();

			let error = send(&directory.path().join("nope.sock"), Request::Status)
				.await
				.unwrap_err();

			assert!(matches!(error, IpcError::NotRunning));
		}

		#[tokio::test]
		async fn a_stale_socket_is_replaced_but_a_live_one_is_not() {
			let directory = tempfile::tempdir().unwrap();
			let socket = directory.path().join("background/updater.sock");

			// A leftover file from a crashed updater.
			std::fs::create_dir_all(socket.parent().unwrap()).unwrap();
			drop(std::os::unix::net::UnixListener::bind(&socket).unwrap());
			assert!(socket.exists());

			let (_socket, _stop) = running(directory.path()).await;

			// Now one is live, so a second updater cannot take it over.
			assert!(bind(&socket).await.is_err());
		}
	}
}

#[cfg(unix)]
pub use unix::{Handler, bind, send, serve};

#[cfg(not(unix))]
pub async fn send(_socket: &std::path::Path, _request: Request) -> Result<Response, IpcError> {
	Err(IpcError::Unsupported)
}
