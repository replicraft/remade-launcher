//! A per-user scheduled task that runs at logon.
//!
//! It is created without administrator rights, runs as the signed-in user at
//! the lowest run level, and is not a service. The 24-hour timer lives inside
//! the updater, so there is no second schedule to double-run.

use std::path::Path;
use std::sync::Arc;

use super::{
	CommandRunner, LaunchSpec, Registrar, RegistrationError, RegistrationState, first_line,
};

const TASK_NAME: &str = "Remade Launcher Updater";

pub struct WindowsRegistrar {
	runner: Arc<dyn CommandRunner>,
	schtasks: String,
}

impl WindowsRegistrar {
	pub fn new(runner: Arc<dyn CommandRunner>) -> Self {
		// An absolute path, so nothing earlier on PATH can stand in for it.
		let root = std::env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".into());

		Self {
			runner,
			schtasks: format!("{root}\\System32\\schtasks.exe"),
		}
	}

	fn schtasks(&self, args: &[&str]) -> Result<super::CommandOutput, RegistrationError> {
		let args: Vec<String> = args.iter().map(|arg| (*arg).to_owned()).collect();

		Ok(self.runner.run(&self.schtasks, &args)?)
	}
}

/// The command line the task runs: the updater and its data folder, each in
/// double quotes. Quotes and control characters cannot appear in a Windows
/// path, and anything that tries is refused rather than escaped.
pub fn task_command(binary: &Path, data_dir: &Path) -> Result<String, RegistrationError> {
	let quote = |path: &Path| -> Result<String, RegistrationError> {
		let text = path.to_string_lossy();

		if text.chars().any(|c| c == '"' || c.is_control()) {
			return Err(RegistrationError::Refused(
				"a path contains a character that cannot be quoted safely".into(),
			));
		}

		Ok(format!("\"{text}\""))
	};

	Ok(format!(
		"{} --data-dir {}",
		quote(binary)?,
		quote(data_dir)?
	))
}

impl Registrar for WindowsRegistrar {
	fn mechanism(&self) -> &'static str {
		"per-user scheduled task at logon"
	}

	fn register(&self, spec: &LaunchSpec) -> Result<(), RegistrationError> {
		let command = task_command(&spec.binary, &spec.data_dir)?;

		std::fs::create_dir_all(spec.data_dir.join("background"))?;

		let created = self.schtasks(&[
			"/Create", "/TN", TASK_NAME, "/SC", "ONLOGON", "/RL", "LIMITED", "/TR", &command, "/F",
		])?;

		if created.success {
			Ok(())
		} else {
			Err(RegistrationError::Refused(first_line(&format!(
				"{} {}",
				created.stderr, created.stdout
			))))
		}
	}

	fn unregister(&self) -> Result<(), RegistrationError> {
		let state = self.state();

		if matches!(state, RegistrationState::NotInstalled) {
			return Ok(());
		}

		let deleted = self.schtasks(&["/Delete", "/TN", TASK_NAME, "/F"])?;

		if deleted.success {
			Ok(())
		} else {
			Err(RegistrationError::Refused(first_line(&deleted.stderr)))
		}
	}

	fn state(&self) -> RegistrationState {
		match self.schtasks(&["/Query", "/TN", TASK_NAME, "/FO", "LIST", "/V"]) {
			Err(error) => RegistrationState::Failed(error.to_string()),
			Ok(output) if !output.success => RegistrationState::NotInstalled,
			Ok(output) => {
				let disabled = output.stdout.lines().any(|line| {
					let line = line.to_lowercase();

					line.contains("scheduled task state") && line.contains("disabled")
				});

				if disabled {
					RegistrationState::DisabledByOs
				} else {
					RegistrationState::Registered
				}
			}
		}
	}

	fn start(&self) -> Result<(), RegistrationError> {
		let output = self.schtasks(&["/Run", "/TN", TASK_NAME])?;

		if output.success {
			Ok(())
		} else {
			Err(RegistrationError::Refused(first_line(&output.stderr)))
		}
	}

	fn stop(&self) -> Result<(), RegistrationError> {
		let output = self.schtasks(&["/End", "/TN", TASK_NAME])?;

		if output.success {
			Ok(())
		} else {
			Err(RegistrationError::Refused(first_line(&output.stderr)))
		}
	}
}

#[cfg(test)]
mod tests {
	use std::path::PathBuf;

	use super::super::testing::{FakeRunner, spec};
	use super::*;

	fn registrar() -> (Arc<FakeRunner>, WindowsRegistrar) {
		let runner = Arc::new(FakeRunner::default());

		(Arc::clone(&runner), WindowsRegistrar::new(runner))
	}

	fn local_spec() -> LaunchSpec {
		let base = tempfile::tempdir().unwrap().keep();

		LaunchSpec {
			binary: PathBuf::from("C:\\Program Files\\Remade\\remade-launcher-daemon.exe"),
			data_dir: base.join("data"),
		}
	}

	#[test]
	fn the_task_command_quotes_both_paths() {
		let command = task_command(&spec().binary, &spec().data_dir).unwrap();

		assert_eq!(
			command,
			"\"/opt/remade/remade-launcher-daemon\" --data-dir \"/home/me/.local/share/Launcher\""
		);
	}

	#[test]
	fn a_path_that_cannot_be_quoted_is_refused() {
		assert!(task_command(Path::new("C:\\a\"b\\d.exe"), Path::new("C:\\data")).is_err());
		assert!(task_command(Path::new("C:\\d.exe"), Path::new("C:\\da\nta")).is_err());
	}

	#[test]
	fn the_task_runs_at_logon_with_the_lowest_run_level() {
		let (runner, registrar) = registrar();

		registrar.register(&local_spec()).unwrap();

		let call = runner.calls().join("\n");
		assert!(call.contains("/Create"), "{call}");
		assert!(
			call.contains("/SC ONLOGON") && call.contains("/RL LIMITED"),
			"{call}"
		);
		assert!(call.contains("schtasks.exe"));
		assert!(
			!call.contains("SYSTEM") && !call.contains("/RU"),
			"no other account, and not the system's"
		);
	}

	#[test]
	fn state_distinguishes_missing_enabled_and_disabled() {
		let (runner, registrar) = registrar();

		runner.answer("schtasks", "/Query", false, "");
		assert_eq!(registrar.state(), RegistrationState::NotInstalled);

		runner.script.lock().unwrap().clear();
		runner.answer(
			"schtasks",
			"/Query",
			true,
			"TaskName: x\nScheduled Task State: Enabled\n",
		);
		assert_eq!(registrar.state(), RegistrationState::Registered);

		runner.script.lock().unwrap().clear();
		runner.answer(
			"schtasks",
			"/Query",
			true,
			"TaskName: x\nScheduled Task State: Disabled\n",
		);
		assert_eq!(registrar.state(), RegistrationState::DisabledByOs);
	}

	#[test]
	fn unregistering_when_nothing_is_registered_does_nothing() {
		let (runner, registrar) = registrar();
		runner.answer("schtasks", "/Query", false, "");

		registrar.unregister().unwrap();

		assert!(!runner.calls().iter().any(|call| call.contains("/Delete")));
	}
}
