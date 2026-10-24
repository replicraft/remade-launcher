//! Controlling the background updater from the CLI and the desktop app.
//!
//! One place decides how switching modes and registering fit together, so
//! the two front ends cannot drift apart: the login item is registered first,
//! so the saved mode never claims something the system has not done, and
//! leaving the background mode removes the registration.

use std::path::PathBuf;

use launcher_core::{Engine, ModeConsent};
use launcher_model::{LauncherError, UpdateMode};
use thiserror::Error;

use crate::ipc::{self, DaemonInfo, IpcError, Request, Response};
use crate::paths::BackgroundPaths;
use crate::registration::{LaunchSpec, RegistrationError, RegistrationState, host_registrar};
use crate::schedule::INTERVAL_SECONDS;

#[derive(Debug, Error)]
pub enum ControlError {
	#[error("the background updater program is not installed next to this one")]
	UpdaterMissing,
	#[error(transparent)]
	Registration(#[from] RegistrationError),
	#[error(transparent)]
	Engine(#[from] LauncherError),
	#[error(transparent)]
	Ipc(#[from] IpcError),
	#[error("the system did not finish the request")]
	Interrupted,
}

/// The background updater's real state, as the operating system and the
/// updater itself report it, never as a saved preference claims.
#[derive(Clone, Debug)]
pub struct BackgroundStatus {
	/// The saved mode is the background one.
	pub wanted: bool,
	pub mechanism: &'static str,
	pub registration: RegistrationState,
	/// Present only if the updater answered on its socket.
	pub running: Option<DaemonInfo>,
	pub interval_seconds: i64,
	/// From the last run, when the updater is not answering.
	pub saved_last_attempted: Option<i64>,
	pub saved_last_completed: Option<i64>,
}

impl BackgroundStatus {
	/// A stable word for the actual state.
	pub fn actual_state(&self) -> &'static str {
		match (&self.running, &self.registration) {
			(Some(_), _) => "running",
			(None, RegistrationState::Registered) => "registered_not_running",
			(None, RegistrationState::PendingApproval) => "pending_os_approval",
			(None, RegistrationState::DisabledByOs) => "disabled_by_os",
			(None, RegistrationState::NotInstalled) => "not_installed",
			(None, RegistrationState::Unsupported(_)) => "unsupported",
			(None, RegistrationState::Failed(_)) => "failed",
		}
	}
}

pub fn paths_for(engine: &Engine) -> BackgroundPaths {
	BackgroundPaths::new(engine.data_directory().root())
}

/// The updater program that ships beside the running program.
pub fn updater_beside_current_exe() -> Option<PathBuf> {
	crate::paths::daemon_binary_beside(&std::env::current_exe().ok()?)
}

async fn blocking<T: Send + 'static>(
	work: impl FnOnce() -> T + Send + 'static,
) -> Result<T, ControlError> {
	tokio::task::spawn_blocking(work)
		.await
		.map_err(|_| ControlError::Interrupted)
}

pub async fn status(engine: &Engine) -> BackgroundStatus {
	let wanted = engine.update_settings().await.mode == UpdateMode::Daemon;
	let (mechanism, registration) = blocking(|| {
		let registrar = host_registrar();

		(registrar.mechanism(), registrar.state())
	})
	.await
	.unwrap_or((
		"unknown",
		RegistrationState::Failed("the check did not finish".into()),
	));

	let running = match ipc::send(&paths_for(engine).socket, Request::Status).await {
		Ok(Response::Status(info)) => Some(info),
		_ => None,
	};

	let saved = |key: &'static str| async move {
		engine
			.state_value(key)
			.await
			.and_then(|value| value.parse::<i64>().ok())
	};

	BackgroundStatus {
		wanted,
		mechanism,
		registration,
		running,
		interval_seconds: INTERVAL_SECONDS,
		saved_last_attempted: saved("daemon.last_attempted").await,
		saved_last_completed: saved("daemon.last_completed").await,
	}
}

pub async fn register(engine: &Engine, binary: Option<PathBuf>) -> Result<(), ControlError> {
	let binary = binary
		.or_else(updater_beside_current_exe)
		.ok_or(ControlError::UpdaterMissing)?;
	let spec = LaunchSpec {
		binary,
		data_dir: engine.data_directory().root().to_path_buf(),
	};

	blocking(move || host_registrar().register(&spec))
		.await?
		.map_err(ControlError::from)
}

pub async fn unregister(engine: &Engine) -> Result<(), ControlError> {
	// Ask a running updater to stop first, so it does not outlive its login item.
	let _ = ipc::send(&paths_for(engine).socket, Request::Stop).await;

	blocking(|| host_registrar().unregister())
		.await?
		.map_err(ControlError::from)
}

pub async fn start() -> Result<(), ControlError> {
	blocking(|| host_registrar().start())
		.await?
		.map_err(ControlError::from)
}

/// Asks a running updater to stop. `false` if none was running.
pub async fn stop(engine: &Engine) -> Result<bool, ControlError> {
	match ipc::send(&paths_for(engine).socket, Request::Stop).await {
		Ok(_) => Ok(true),
		Err(IpcError::NotRunning | IpcError::Unsupported) => Ok(false),
		Err(error) => Err(error.into()),
	}
}

/// Asks a running updater to scan now. `None` if none was running.
pub async fn check_now(engine: &Engine) -> Result<Option<String>, ControlError> {
	match ipc::send(&paths_for(engine).socket, Request::CheckNow).await {
		Ok(Response::Accepted { detail }) => Ok(Some(detail)),
		Ok(Response::Refused { reason }) => Err(ControlError::Ipc(IpcError::Io(
			std::io::Error::other(reason),
		))),
		Ok(Response::Status(_)) | Err(IpcError::NotRunning | IpcError::Unsupported) => Ok(None),
		Err(error) => Err(error.into()),
	}
}

/// Switches the global update mode, doing the system registration the mode
/// needs.
///
/// Moving to the background mode registers the login item first. If saving
/// the mode then fails, the registration is undone, so the two always agree.
/// Moving away from it removes the registration. Nothing here starts an
/// update.
pub async fn apply_mode(
	engine: &Engine,
	mode: UpdateMode,
	consent: &ModeConsent,
	binary: Option<PathBuf>,
) -> Result<(), ControlError> {
	let spec = match mode {
		UpdateMode::Daemon => {
			let binary = binary
				.or_else(updater_beside_current_exe)
				.ok_or(ControlError::UpdaterMissing)?;

			Some(LaunchSpec {
				binary,
				data_dir: engine.data_directory().root().to_path_buf(),
			})
		}
		_ => None,
	};

	apply_mode_with(
		engine,
		mode,
		consent,
		spec,
		std::sync::Arc::from(host_registrar()),
	)
	.await
}

pub(crate) async fn apply_mode_with(
	engine: &Engine,
	mode: UpdateMode,
	consent: &ModeConsent,
	spec: Option<LaunchSpec>,
	registrar: std::sync::Arc<dyn crate::registration::Registrar>,
) -> Result<(), ControlError> {
	let previous = engine.update_settings().await.mode;

	if let Some(spec) = spec {
		// The engine checks consent too, but registering first would leave a
		// login item behind if it then refused, so check before touching the
		// system.
		if !consent.acknowledged_risks {
			return Err(ControlError::Engine(LauncherError::new(
				launcher_model::ErrorCode::ConsentRequired,
				"Automatic updates can install a bad release without asking. Confirm that you understand before turning them on.",
			)));
		}

		let registrar = std::sync::Arc::clone(&registrar);

		blocking(move || registrar.register(&spec)).await??;
	}

	if let Err(error) = engine.set_update_mode(mode, consent).await {
		if mode == UpdateMode::Daemon && previous != UpdateMode::Daemon {
			let registrar = std::sync::Arc::clone(&registrar);
			let _ = blocking(move || registrar.unregister()).await;
		}

		return Err(error.into());
	}

	if previous == UpdateMode::Daemon && mode != UpdateMode::Daemon {
		let _ = ipc::send(&paths_for(engine).socket, Request::Stop).await;

		blocking(move || registrar.unregister()).await??;
	}

	Ok(())
}

#[cfg(test)]
mod tests {
	use std::sync::Mutex;

	use launcher_core::DataDirectory;

	use super::*;
	use crate::registration::Registrar;

	#[derive(Default)]
	struct FakeRegistrar {
		calls: Mutex<Vec<&'static str>>,
		refuse_register: bool,
	}

	impl Registrar for FakeRegistrar {
		fn mechanism(&self) -> &'static str {
			"fake"
		}

		fn register(&self, _: &LaunchSpec) -> Result<(), RegistrationError> {
			self.calls.lock().unwrap().push("register");

			if self.refuse_register {
				Err(RegistrationError::Refused("no".into()))
			} else {
				Ok(())
			}
		}

		fn unregister(&self) -> Result<(), RegistrationError> {
			self.calls.lock().unwrap().push("unregister");

			Ok(())
		}

		fn state(&self) -> RegistrationState {
			RegistrationState::NotInstalled
		}

		fn start(&self) -> Result<(), RegistrationError> {
			Ok(())
		}

		fn stop(&self) -> Result<(), RegistrationError> {
			Ok(())
		}
	}

	async fn engine() -> (tempfile::TempDir, Engine) {
		let directory = tempfile::tempdir().unwrap();
		let engine = Engine::open(&DataDirectory::at(directory.path().join("data")))
			.await
			.unwrap();

		(directory, engine)
	}

	fn spec() -> Option<LaunchSpec> {
		Some(LaunchSpec {
			binary: PathBuf::from("/x/daemon"),
			data_dir: PathBuf::from("/x/data"),
		})
	}

	fn consent(yes: bool) -> ModeConsent {
		ModeConsent {
			actor: "test".into(),
			acknowledged_risks: yes,
		}
	}

	#[tokio::test]
	async fn the_background_mode_registers_first_and_then_saves() {
		let (_directory, engine) = engine().await;
		let registrar = std::sync::Arc::new(FakeRegistrar::default());

		apply_mode_with(
			&engine,
			UpdateMode::Daemon,
			&consent(true),
			spec(),
			registrar.clone(),
		)
		.await
		.unwrap();

		assert_eq!(engine.update_settings().await.mode, UpdateMode::Daemon);
		assert_eq!(*registrar.calls.lock().unwrap(), vec!["register"]);
	}

	#[tokio::test]
	async fn a_refused_registration_leaves_the_mode_alone() {
		let (_directory, engine) = engine().await;
		let registrar = std::sync::Arc::new(FakeRegistrar {
			refuse_register: true,
			..FakeRegistrar::default()
		});

		let error = apply_mode_with(
			&engine,
			UpdateMode::Daemon,
			&consent(true),
			spec(),
			registrar,
		)
		.await
		.unwrap_err();

		assert!(matches!(error, ControlError::Registration(_)));
		assert_eq!(
			engine.update_settings().await.mode,
			UpdateMode::Manual,
			"the saved mode must not claim what the system refused"
		);
	}

	#[tokio::test]
	async fn without_an_acknowledgement_nothing_touches_the_system() {
		let (_directory, engine) = engine().await;
		let registrar = std::sync::Arc::new(FakeRegistrar::default());

		let error = apply_mode_with(
			&engine,
			UpdateMode::Daemon,
			&consent(false),
			spec(),
			registrar.clone(),
		)
		.await
		.unwrap_err();

		assert!(matches!(error, ControlError::Engine(_)));
		assert!(
			registrar.calls.lock().unwrap().is_empty(),
			"no login item may be created without consent"
		);
		assert_eq!(engine.update_settings().await.mode, UpdateMode::Manual);
	}

	#[tokio::test]
	async fn leaving_the_background_mode_removes_the_registration() {
		let (_directory, engine) = engine().await;
		let registrar = std::sync::Arc::new(FakeRegistrar::default());

		apply_mode_with(
			&engine,
			UpdateMode::Daemon,
			&consent(true),
			spec(),
			registrar.clone(),
		)
		.await
		.unwrap();
		apply_mode_with(
			&engine,
			UpdateMode::Manual,
			&consent(false),
			None,
			registrar.clone(),
		)
		.await
		.unwrap();

		assert_eq!(engine.update_settings().await.mode, UpdateMode::Manual);
		assert_eq!(
			*registrar.calls.lock().unwrap(),
			vec!["register", "unregister"]
		);
	}

	#[tokio::test]
	async fn the_on_launch_mode_needs_no_registration_at_all() {
		let (_directory, engine) = engine().await;
		let registrar = std::sync::Arc::new(FakeRegistrar::default());

		apply_mode_with(
			&engine,
			UpdateMode::OnLaunch,
			&consent(true),
			None,
			registrar.clone(),
		)
		.await
		.unwrap();

		assert_eq!(engine.update_settings().await.mode, UpdateMode::OnLaunch);
		assert!(registrar.calls.lock().unwrap().is_empty());
	}
}
