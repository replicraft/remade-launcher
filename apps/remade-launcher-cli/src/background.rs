//! The CLI's view of the background updater.
//!
//! Status is asked of the operating system and of the updater itself. It is
//! never read back from the saved preference, so "enabled" and "running"
//! mean what they say.

use std::sync::Arc;

use launcher_background::ipc::{self, DaemonInfo, IpcError, Request, Response};
use launcher_background::registration::RegistrationError;
use launcher_background::{
	BackgroundPaths, LaunchSpec, RegistrationState, host_registrar, paths::daemon_binary_beside,
};
use launcher_core::Engine;
use launcher_model::UpdateMode;
use serde_json::{Value, json};

use crate::errors::CliError;
use crate::exit::ExitStatus;
use crate::output::iso;

fn paths(engine: &Engine) -> BackgroundPaths {
	BackgroundPaths::new(engine.data_directory().root())
}

fn spec(engine: &Engine) -> Result<LaunchSpec, CliError> {
	let exe = std::env::current_exe()
		.map_err(|_| unsupported("The launcher could not find its own program."))?;
	let binary = daemon_binary_beside(&exe).ok_or_else(|| {
		CliError::new(
			"DAEMON_REGISTRATION_FAILED",
			ExitStatus::Unsupported,
			"The background updater program (remade-launcher-daemon) is not installed next to this command.",
		)
	})?;

	Ok(LaunchSpec {
		binary,
		data_dir: engine.data_directory().root().to_path_buf(),
	})
}

fn unsupported(message: &str) -> CliError {
	CliError::new("AUTO_START_UNSUPPORTED", ExitStatus::Unsupported, message)
}

fn from_registration(error: RegistrationError) -> CliError {
	match error {
		RegistrationError::Unsupported(why) => CliError::new(
			"AUTO_START_UNSUPPORTED",
			ExitStatus::Unsupported,
			format!("This system cannot start the updater at login: {why}"),
		),
		RegistrationError::Refused(why) => CliError::new(
			"DAEMON_REGISTRATION_FAILED",
			ExitStatus::Unsupported,
			format!("The system refused to register the updater: {why}"),
		),
		RegistrationError::Io(error) => CliError::new(
			"DAEMON_REGISTRATION_FAILED",
			ExitStatus::Internal,
			format!("The login item could not be written: {error}"),
		),
	}
}

/// Runs a blocking registrar call off the async threads.
async fn registrar_call<T: Send + 'static>(
	work: impl FnOnce(&dyn launcher_background::Registrar) -> T + Send + 'static,
) -> Result<T, CliError> {
	tokio::task::spawn_blocking(move || work(host_registrar().as_ref()))
		.await
		.map_err(|_| {
			CliError::new(
				"INTERNAL",
				ExitStatus::Internal,
				"The registration check did not finish.",
			)
		})
}

pub(crate) async fn register(engine: &Arc<Engine>) -> Result<(), CliError> {
	let spec = spec(engine)?;

	registrar_call(move |registrar| registrar.register(&spec))
		.await?
		.map_err(from_registration)
}

pub(crate) async fn unregister(engine: &Arc<Engine>) -> Result<(), CliError> {
	// Ask a running updater to stop first, so it does not outlive its login item.
	let _ = ipc::send(&paths(engine).socket, Request::Stop).await;

	registrar_call(|registrar| registrar.unregister())
		.await?
		.map_err(from_registration)
}

pub(crate) async fn start() -> Result<(), CliError> {
	registrar_call(|registrar| registrar.start())
		.await?
		.map_err(from_registration)
}

pub(crate) async fn running(engine: &Engine) -> Result<Option<DaemonInfo>, CliError> {
	match ipc::send(&paths(engine).socket, Request::Status).await {
		Ok(Response::Status(info)) => Ok(Some(info)),
		Ok(_) => Err(CliError::new(
			"DAEMON_REGISTRATION_FAILED",
			ExitStatus::Internal,
			"The updater sent an unexpected reply.",
		)),
		Err(IpcError::NotRunning | IpcError::Unsupported) => Ok(None),
		Err(error) => Err(CliError::new(
			"DAEMON_REGISTRATION_FAILED",
			ExitStatus::Internal,
			error.to_string(),
		)),
	}
}

pub(crate) async fn stop(engine: &Engine) -> Result<bool, CliError> {
	match ipc::send(&paths(engine).socket, Request::Stop).await {
		Ok(_) => Ok(true),
		Err(IpcError::NotRunning | IpcError::Unsupported) => Ok(false),
		Err(error) => Err(CliError::new(
			"DAEMON_REGISTRATION_FAILED",
			ExitStatus::Internal,
			error.to_string(),
		)),
	}
}

/// `Some(detail)` if a running updater took the request.
pub(crate) async fn check_now(engine: &Engine) -> Result<Option<String>, CliError> {
	match ipc::send(&paths(engine).socket, Request::CheckNow).await {
		Ok(Response::Accepted { detail }) => Ok(Some(detail)),
		Ok(Response::Refused { reason }) => Err(CliError::new(
			"DAEMON_REGISTRATION_FAILED",
			ExitStatus::Conflict,
			reason,
		)),
		Ok(Response::Status(_)) => Ok(None),
		Err(IpcError::NotRunning | IpcError::Unsupported) => Ok(None),
		Err(error) => Err(CliError::new(
			"DAEMON_REGISTRATION_FAILED",
			ExitStatus::Internal,
			error.to_string(),
		)),
	}
}

pub(crate) fn logs(engine: &Engine, tail: usize) -> Vec<String> {
	let text = std::fs::read_to_string(paths(engine).log).unwrap_or_default();
	let lines: Vec<&str> = text.lines().collect();

	lines[lines.len().saturating_sub(tail)..]
		.iter()
		.map(|line| (*line).to_owned())
		.collect()
}

/// The daemon's state as the contract describes it. `actual_state` comes from
/// asking the OS and the updater.
pub(crate) async fn describe(engine: &Arc<Engine>) -> Value {
	let mode = engine.update_settings().await.mode;
	let registration = registrar_call(|registrar| (registrar.mechanism(), registrar.state())).await;
	let live = running(engine).await.ok().flatten();

	let (mechanism, state) = registration.unwrap_or((
		"unknown",
		RegistrationState::Failed("the check did not finish".into()),
	));

	let actual = match (&live, &state) {
		(Some(_), _) => "running",
		(None, RegistrationState::Registered) => "registered_not_running",
		(None, RegistrationState::PendingApproval) => "pending_os_approval",
		(None, RegistrationState::DisabledByOs) => "disabled_by_os",
		(None, RegistrationState::NotInstalled) => "not_installed",
		(None, RegistrationState::Unsupported(_)) => "unsupported",
		(None, RegistrationState::Failed(_)) => "failed",
	};

	let saved = |key: &'static str| async move {
		engine
			.state_value(key)
			.await
			.and_then(|value| value.parse::<i64>().ok())
	};

	let (attempted, completed, due) = match &live {
		Some(info) => (info.last_attempted, info.last_completed, info.next_due),
		None => (
			saved("daemon.last_attempted").await,
			saved("daemon.last_completed").await,
			saved("daemon.next_due").await,
		),
	};

	json!({
		"desired_state": if mode == UpdateMode::Daemon { "enabled" } else { "disabled" },
		"actual_state": actual,
		"registration_type": mechanism,
		"registration_detail": match &state {
			RegistrationState::Failed(why) | RegistrationState::Unsupported(why) => Some(why.clone()),
			_ => None,
		},
		"check_interval_seconds": launcher_background::schedule::INTERVAL_SECONDS,
		"last_attempted_check": attempted.map(iso),
		"last_successful_check": completed.map(iso),
		"next_due_check": if actual == "running" { due.map(iso) } else { None },
		"last_result": live.as_ref().and_then(|info| info.last_outcome.clone()),
		"queue_length": live.as_ref().map(|info| info.queue_length),
		"pid": live.as_ref().map(|info| info.pid),
	})
}
