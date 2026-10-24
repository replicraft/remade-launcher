//! The CLI's view of the background updater.
//!
//! The work lives in `launcher_background::control`, shared with the desktop
//! app. This file only turns its errors into the CLI's typed ones and its
//! status into the JSON the contract describes.

use std::sync::Arc;

use launcher_background::control::{self, ControlError};
use launcher_background::ipc::IpcError;
use launcher_background::registration::{RegistrationError, RegistrationState};
use launcher_core::Engine;
use serde_json::{Value, json};

use crate::errors::CliError;
use crate::exit::ExitStatus;
use crate::output::iso;

pub(crate) fn cli_error(error: ControlError) -> CliError {
	match error {
		ControlError::UpdaterMissing => CliError::new(
			"DAEMON_REGISTRATION_FAILED",
			ExitStatus::Unsupported,
			"The background updater program (remade-launcher-daemon) is not installed next to this command.",
		),
		ControlError::Registration(RegistrationError::Unsupported(why)) => CliError::new(
			"AUTO_START_UNSUPPORTED",
			ExitStatus::Unsupported,
			format!("This system cannot start the updater at login: {why}"),
		),
		ControlError::Registration(RegistrationError::Refused(why)) => CliError::new(
			"DAEMON_REGISTRATION_FAILED",
			ExitStatus::Unsupported,
			format!("The system refused to register the updater: {why}"),
		),
		ControlError::Registration(RegistrationError::Io(error)) => CliError::new(
			"DAEMON_REGISTRATION_FAILED",
			ExitStatus::Internal,
			format!("The login item could not be written: {error}"),
		),
		ControlError::Engine(error) => error.into(),
		ControlError::Ipc(IpcError::TimedOut) => CliError::new(
			"DAEMON_NOT_RESPONDING",
			ExitStatus::Network,
			"The updater did not answer in time.",
		),
		ControlError::Ipc(error) => CliError::new(
			"DAEMON_REGISTRATION_FAILED",
			ExitStatus::Internal,
			error.to_string(),
		),
		ControlError::Interrupted => CliError::new(
			"INTERNAL",
			ExitStatus::Internal,
			"The system did not finish the request.",
		),
	}
}

pub(crate) async fn unregister(engine: &Arc<Engine>) -> Result<(), CliError> {
	control::unregister(engine).await.map_err(cli_error)
}

pub(crate) async fn start() -> Result<(), CliError> {
	control::start().await.map_err(cli_error)
}

pub(crate) async fn stop(engine: &Engine) -> Result<bool, CliError> {
	control::stop(engine).await.map_err(cli_error)
}

pub(crate) async fn check_now(engine: &Engine) -> Result<Option<String>, CliError> {
	control::check_now(engine).await.map_err(cli_error)
}

pub(crate) fn logs(engine: &Engine, tail: usize) -> Vec<String> {
	let text = std::fs::read_to_string(control::paths_for(engine).log).unwrap_or_default();
	let lines: Vec<&str> = text.lines().collect();

	lines[lines.len().saturating_sub(tail)..]
		.iter()
		.map(|line| (*line).to_owned())
		.collect()
}

/// The daemon's state as the contract describes it. `actual_state` comes
/// from asking the OS and the updater.
pub(crate) async fn describe(engine: &Arc<Engine>) -> Value {
	let status = control::status(engine).await;

	let (attempted, completed, due) = match &status.running {
		Some(info) => (info.last_attempted, info.last_completed, info.next_due),
		None => (
			status.saved_last_attempted,
			status.saved_last_completed,
			None,
		),
	};

	json!({
		"desired_state": if status.wanted { "enabled" } else { "disabled" },
		"actual_state": status.actual_state(),
		"registration_type": status.mechanism,
		"registration_detail": match &status.registration {
			RegistrationState::Failed(why) | RegistrationState::Unsupported(why) => Some(why.clone()),
			_ => None,
		},
		"check_interval_seconds": status.interval_seconds,
		"last_attempted_check": attempted.map(iso),
		"last_successful_check": completed.map(iso),
		"next_due_check": due.map(iso),
		"last_result": status.running.as_ref().and_then(|info| info.last_outcome.clone()),
		"queue_length": status.running.as_ref().map(|info| info.queue_length),
		"pid": status.running.as_ref().map(|info| info.pid),
	})
}
