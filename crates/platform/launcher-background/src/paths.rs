use std::path::{Path, PathBuf};

use crate::DAEMON_BINARY;

/// The longest socket path that works on every Unix. macOS allows 104 bytes
/// and Linux 108, counting the terminator.
const MAX_SOCKET_PATH: usize = 100;

/// Where the updater keeps its files.
///
/// The log and lock live in the launcher's data folder. The socket does not:
/// a Unix socket path has a hard length limit that a long home folder can
/// exceed, so it goes in a short per-user folder under the system's
/// temporary directory, named by a hash of the data folder so each launcher
/// profile has its own.
#[derive(Clone, Debug)]
pub struct BackgroundPaths {
	pub socket: PathBuf,
	pub log: PathBuf,
	pub lock: PathBuf,
}

/// A stable 64-bit hash of `text`. FNV-1a, written out so the CLI, the
/// desktop app, and the updater always agree, whatever Rust version built
/// each one.
fn fingerprint(text: &str) -> u64 {
	text.bytes().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
		(hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3)
	})
}

impl BackgroundPaths {
	pub fn new(data_root: &Path) -> Self {
		let folder = data_root.join("background");
		let name = format!("remade-{:016x}", fingerprint(&data_root.to_string_lossy()));

		let in_temp = std::env::temp_dir().join(&name).join("updater.sock");
		let socket = if in_temp.as_os_str().len() <= MAX_SOCKET_PATH {
			in_temp
		} else {
			PathBuf::from("/tmp").join(&name).join("updater.sock")
		};

		Self {
			socket,
			log: folder.join("updater.log"),
			lock: folder.join("updater.lock"),
		}
	}

	/// The folder for the log and lock.
	pub fn folder(&self) -> &Path {
		self.log.parent().unwrap_or(Path::new("."))
	}
}

/// The updater executable that sits beside `current_exe`, if there is one.
/// It is looked for only next to the program that asks, never on `PATH`, so
/// nothing else can stand in for it.
pub fn daemon_binary_beside(current_exe: &Path) -> Option<PathBuf> {
	let name = if cfg!(windows) {
		format!("{DAEMON_BINARY}.exe")
	} else {
		DAEMON_BINARY.to_owned()
	};
	let candidate = current_exe.parent()?.join(name);

	candidate.is_file().then_some(candidate)
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn the_log_and_lock_live_in_the_data_folder() {
		let paths = BackgroundPaths::new(Path::new("/data/remade"));

		assert_eq!(paths.folder(), Path::new("/data/remade/background"));
		assert!(paths.log.starts_with(paths.folder()) && paths.lock.starts_with(paths.folder()));
	}

	#[test]
	fn the_socket_path_is_short_even_when_the_data_folder_is_very_long() {
		let long = PathBuf::from(format!("/{}", "deep/".repeat(60)));
		let paths = BackgroundPaths::new(&long);

		assert!(
			paths.socket.as_os_str().len() <= MAX_SOCKET_PATH,
			"{}",
			paths.socket.display()
		);
		assert!(paths.socket.ends_with("updater.sock"));
	}

	#[test]
	fn each_data_folder_gets_its_own_stable_socket() {
		let a = BackgroundPaths::new(Path::new("/data/a"));
		let b = BackgroundPaths::new(Path::new("/data/b"));

		assert_ne!(a.socket, b.socket);
		assert_eq!(a.socket, BackgroundPaths::new(Path::new("/data/a")).socket);
		// The empty string hashes to the FNV-1a offset basis, which pins the algorithm.
		assert_eq!(fingerprint(""), 0xcbf2_9ce4_8422_2325);
		assert_eq!(fingerprint("a"), 0xaf63_dc4c_8601_ec8c);
	}

	#[test]
	fn the_updater_is_only_found_beside_the_caller() {
		let directory = tempfile::tempdir().unwrap();
		let exe = directory.path().join("remade");

		assert_eq!(daemon_binary_beside(&exe), None);

		let name = if cfg!(windows) {
			"remade-launcher-daemon.exe"
		} else {
			"remade-launcher-daemon"
		};
		std::fs::write(directory.path().join(name), "").unwrap();

		assert_eq!(
			daemon_binary_beside(&exe),
			Some(directory.path().join(name))
		);
	}
}
