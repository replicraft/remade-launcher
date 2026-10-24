//! Linux and the other Unix desktops.
//!
//! With a systemd user manager, the updater is a user service. Without one,
//! it is an XDG autostart entry, which any freedesktop session starts at
//! login. A machine with neither, such as a bare SSH host, gets an honest
//! "unsupported" and instructions to schedule it by hand.

use std::path::PathBuf;
use std::sync::Arc;

use super::{
	CommandRunner, LaunchSpec, Registrar, RegistrationError, RegistrationState, first_line,
};
use crate::SERVICE_LABEL;

const UNIT_NAME: &str = "remade-launcher-updater.service";

pub struct LinuxRegistrar {
	runner: Arc<dyn CommandRunner>,
	config_home: PathBuf,
	/// Whether this session has a graphical desktop that would run autostart
	/// entries.
	desktop_session: bool,
	/// Where to find `systemctl`, if the system has one.
	systemctl: Option<String>,
}

impl LinuxRegistrar {
	pub fn new(
		runner: Arc<dyn CommandRunner>,
		config_home: PathBuf,
		desktop_session: bool,
		systemctl: Option<String>,
	) -> Self {
		Self {
			runner,
			config_home,
			desktop_session,
			systemctl,
		}
	}

	pub fn for_current_user(runner: Arc<dyn CommandRunner>) -> Self {
		let config_home = std::env::var_os("XDG_CONFIG_HOME")
			.map(PathBuf::from)
			.filter(|path| path.is_absolute())
			.or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
			.unwrap_or_default();

		let desktop_session =
			std::env::var_os("XDG_CURRENT_DESKTOP").is_some_and(|value| !value.is_empty());

		let systemctl = ["/usr/bin/systemctl", "/bin/systemctl"]
			.into_iter()
			.find(|path| std::path::Path::new(path).is_file())
			.map(str::to_owned)
			// A user manager that is not running cannot hold a unit.
			.filter(|path| {
				runner
					.run(path, &["--user".into(), "show-environment".into()])
					.is_ok_and(|output| output.success)
			});

		Self::new(runner, config_home, desktop_session, systemctl)
	}

	pub fn unit_path(&self) -> PathBuf {
		self.config_home.join("systemd/user").join(UNIT_NAME)
	}

	pub fn autostart_path(&self) -> PathBuf {
		self.config_home
			.join("autostart")
			.join(format!("{SERVICE_LABEL}.desktop"))
	}

	fn systemctl(&self, args: &[&str]) -> Result<super::CommandOutput, RegistrationError> {
		let program = self
			.systemctl
			.as_deref()
			.ok_or_else(|| RegistrationError::Unsupported("no systemd".into()))?;
		let mut full: Vec<String> = vec!["--user".into()];

		full.extend(args.iter().map(|arg| (*arg).to_owned()));

		Ok(self.runner.run(program, &full)?)
	}
}

/// Quotes one argument for a systemd `ExecStart=` line.
///
/// systemd splits on whitespace and treats `\`, `"`, `'`, `%` (specifiers),
/// and `$` (variables) specially, so each is escaped inside double quotes.
pub fn systemd_quote(argument: &str) -> String {
	let mut quoted = String::with_capacity(argument.len() + 2);

	quoted.push('"');

	for character in argument.chars() {
		match character {
			'\\' => quoted.push_str("\\\\"),
			'"' => quoted.push_str("\\\""),
			'%' => quoted.push_str("%%"),
			'$' => quoted.push_str("$$"),
			'\n' | '\r' => quoted.push(' '),
			other => quoted.push(other),
		}
	}

	quoted.push('"');

	quoted
}

/// Quotes one argument for the `Exec=` key of a desktop entry.
///
/// The Desktop Entry spec reserves a set of characters, which force quotes,
/// and inside quotes `"`, `` ` ``, `$`, and `\` take a backslash. `%` is a
/// field code, so a literal one is doubled.
pub fn xdg_quote(argument: &str) -> String {
	let mut quoted = String::with_capacity(argument.len() + 2);

	quoted.push('"');

	for character in argument.chars() {
		match character {
			'"' | '`' | '$' | '\\' => {
				quoted.push('\\');
				quoted.push(character);
			}
			'%' => quoted.push_str("%%"),
			'\n' | '\r' => quoted.push(' '),
			other => quoted.push(other),
		}
	}

	quoted.push('"');

	quoted
}

fn unit_file(spec: &LaunchSpec) -> String {
	format!(
		"[Unit]\nDescription=Remade Launcher background updater\n\n[Service]\nType=simple\nExecStart={} --data-dir {}\nRestart=no\nNice=10\nIOSchedulingClass=idle\n\n[Install]\nWantedBy=default.target\n",
		systemd_quote(&spec.binary.to_string_lossy()),
		systemd_quote(&spec.data_dir.to_string_lossy()),
	)
}

fn desktop_file(spec: &LaunchSpec) -> String {
	format!(
		"[Desktop Entry]\nType=Application\nName=Remade Launcher updater\nComment=Checks for app updates in the background\nExec={} --data-dir {}\nTerminal=false\nNoDisplay=true\nX-GNOME-Autostart-enabled=true\n",
		xdg_quote(&spec.binary.to_string_lossy()),
		xdg_quote(&spec.data_dir.to_string_lossy()),
	)
}

impl Registrar for LinuxRegistrar {
	fn mechanism(&self) -> &'static str {
		if self.systemctl.is_some() {
			"systemd user service"
		} else {
			"XDG autostart entry"
		}
	}

	fn register(&self, spec: &LaunchSpec) -> Result<(), RegistrationError> {
		if self.config_home.as_os_str().is_empty() {
			return Err(RegistrationError::Unsupported(
				"the user's configuration folder could not be found".into(),
			));
		}

		std::fs::create_dir_all(spec.data_dir.join("background"))?;

		if self.systemctl.is_some() {
			let unit = self.unit_path();

			if let Some(folder) = unit.parent() {
				std::fs::create_dir_all(folder)?;
			}

			std::fs::write(&unit, unit_file(spec))?;
			self.systemctl(&["daemon-reload"])?;

			let enabled = self.systemctl(&["enable", "--now", UNIT_NAME])?;

			return if enabled.success {
				Ok(())
			} else {
				Err(RegistrationError::Refused(first_line(&enabled.stderr)))
			};
		}

		if !self.desktop_session {
			return Err(RegistrationError::Unsupported(
				"there is no systemd user manager and no desktop session, so nothing would start it at login. Schedule `remade-launcher-daemon` yourself, for example with cron's @reboot.".into(),
			));
		}

		let entry = self.autostart_path();

		if let Some(folder) = entry.parent() {
			std::fs::create_dir_all(folder)?;
		}

		std::fs::write(entry, desktop_file(spec))?;

		Ok(())
	}

	fn unregister(&self) -> Result<(), RegistrationError> {
		if self.systemctl.is_some() && self.unit_path().is_file() {
			let _ = self.systemctl(&["disable", "--now", UNIT_NAME]);
		}

		for path in [self.unit_path(), self.autostart_path()] {
			match std::fs::remove_file(path) {
				Ok(()) => {}
				Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
				Err(error) => return Err(error.into()),
			}
		}

		if self.systemctl.is_some() {
			let _ = self.systemctl(&["daemon-reload"]);
		}

		Ok(())
	}

	fn state(&self) -> RegistrationState {
		if self.systemctl.is_some() && self.unit_path().is_file() {
			return match self.systemctl(&["is-enabled", UNIT_NAME]) {
				Ok(output) if output.stdout.trim() == "enabled" => RegistrationState::Registered,
				// It exists but was switched off outside the launcher.
				Ok(output) if output.stdout.trim() == "disabled" => RegistrationState::DisabledByOs,
				Ok(output) => RegistrationState::Failed(first_line(&output.stdout)),
				Err(error) => RegistrationState::Failed(error.to_string()),
			};
		}

		if self.autostart_path().is_file() {
			return RegistrationState::Registered;
		}

		if self.systemctl.is_none() && !self.desktop_session {
			return RegistrationState::Unsupported(
				"no systemd user manager and no desktop session".into(),
			);
		}

		RegistrationState::NotInstalled
	}

	fn start(&self) -> Result<(), RegistrationError> {
		let output = self.systemctl(&["start", UNIT_NAME])?;

		if output.success {
			Ok(())
		} else {
			Err(RegistrationError::Refused(first_line(&output.stderr)))
		}
	}

	fn stop(&self) -> Result<(), RegistrationError> {
		let output = self.systemctl(&["stop", UNIT_NAME])?;

		if output.success {
			Ok(())
		} else {
			Err(RegistrationError::Refused(first_line(&output.stderr)))
		}
	}
}

#[cfg(test)]
mod tests {
	use super::super::testing::{FakeRunner, spec};
	use super::*;

	fn with_systemd(config: &std::path::Path) -> (Arc<FakeRunner>, LinuxRegistrar) {
		let runner = Arc::new(FakeRunner::default());

		(
			Arc::clone(&runner),
			LinuxRegistrar::new(
				runner,
				config.to_path_buf(),
				true,
				Some("/usr/bin/systemctl".into()),
			),
		)
	}

	fn data_dir(base: &std::path::Path) -> LaunchSpec {
		LaunchSpec {
			binary: spec().binary,
			data_dir: base.join("data"),
		}
	}

	#[test]
	fn quoting_defuses_spaces_specifiers_and_variables() {
		assert_eq!(systemd_quote("/opt/a b/daemon"), "\"/opt/a b/daemon\"");
		assert_eq!(systemd_quote("100%$HOME\"x\\"), "\"100%%$$HOME\\\"x\\\\\"");
		assert_eq!(systemd_quote("a\nb"), "\"a b\"");

		assert_eq!(xdg_quote("/opt/a b/daemon"), "\"/opt/a b/daemon\"");
		assert_eq!(
			xdg_quote("$HOME`x`\"y\"%f"),
			"\"\\$HOME\\`x\\`\\\"y\\\"%%f\""
		);
	}

	#[test]
	fn the_unit_is_a_user_service_that_does_not_restart_itself() {
		let unit = unit_file(&spec());

		assert!(unit.contains("WantedBy=default.target"));
		assert!(unit.contains("Restart=no"));
		assert!(unit.contains(
			"ExecStart=\"/opt/remade/remade-launcher-daemon\" --data-dir \"/home/me/.local/share/Launcher\""
		));
		assert!(!unit.contains("User=") && !unit.contains("multi-user.target"));
	}

	#[test]
	fn systemd_registration_enables_and_starts_a_user_unit() {
		let config = tempfile::tempdir().unwrap();
		let (runner, registrar) = with_systemd(config.path());

		registrar.register(&data_dir(config.path())).unwrap();

		assert!(registrar.unit_path().is_file());

		let calls = runner.calls().join("\n");
		assert!(calls.contains("--user daemon-reload"), "{calls}");
		assert!(
			calls.contains("--user enable --now remade-launcher-updater.service"),
			"{calls}"
		);
	}

	#[test]
	fn state_reports_enabled_disabled_and_missing_truthfully() {
		let config = tempfile::tempdir().unwrap();
		let (runner, registrar) = with_systemd(config.path());

		assert_eq!(registrar.state(), RegistrationState::NotInstalled);

		registrar.register(&data_dir(config.path())).unwrap();
		runner.answer("systemctl", "is-enabled", true, "enabled\n");
		assert_eq!(registrar.state(), RegistrationState::Registered);

		runner.script.lock().unwrap().clear();
		runner.answer("systemctl", "is-enabled", false, "disabled\n");
		assert_eq!(registrar.state(), RegistrationState::DisabledByOs);
	}

	#[test]
	fn without_systemd_a_desktop_session_gets_an_autostart_entry() {
		let config = tempfile::tempdir().unwrap();
		let registrar = LinuxRegistrar::new(
			Arc::new(FakeRunner::default()),
			config.path().to_path_buf(),
			true,
			None,
		);

		registrar.register(&data_dir(config.path())).unwrap();

		let entry = std::fs::read_to_string(registrar.autostart_path()).unwrap();
		assert!(entry.contains("Exec=\"/opt/remade/remade-launcher-daemon\""));
		assert_eq!(registrar.state(), RegistrationState::Registered);
		assert_eq!(registrar.mechanism(), "XDG autostart entry");

		registrar.unregister().unwrap();
		assert_eq!(registrar.state(), RegistrationState::NotInstalled);
	}

	#[test]
	fn a_headless_machine_is_unsupported_and_nothing_is_written() {
		let config = tempfile::tempdir().unwrap();
		let registrar = LinuxRegistrar::new(
			Arc::new(FakeRunner::default()),
			config.path().to_path_buf(),
			false,
			None,
		);

		let error = registrar.register(&data_dir(config.path())).unwrap_err();

		assert!(matches!(error, RegistrationError::Unsupported(_)));
		assert!(error.to_string().contains("cron"));
		assert!(matches!(
			registrar.state(),
			RegistrationState::Unsupported(_)
		));
		assert!(!registrar.autostart_path().exists());
	}

	#[test]
	fn unregistering_removes_both_kinds_of_entry_and_is_repeatable() {
		let config = tempfile::tempdir().unwrap();
		let (_runner, registrar) = with_systemd(config.path());
		registrar.register(&data_dir(config.path())).unwrap();
		std::fs::create_dir_all(registrar.autostart_path().parent().unwrap()).unwrap();
		std::fs::write(registrar.autostart_path(), "x").unwrap();

		registrar.unregister().unwrap();
		registrar.unregister().unwrap();

		assert!(!registrar.unit_path().exists() && !registrar.autostart_path().exists());
	}
}
