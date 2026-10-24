//! A per-user LaunchAgent.
//!
//! This is a plist in the user's own `~/Library/LaunchAgents`, loaded with
//! `launchctl` into the user's login session. It is not a root LaunchDaemon.
//! The system's preferred route for an app that ships a signed bundle is
//! `SMAppService`; that needs the bundled helper layout, so this plain
//! LaunchAgent is the route for now and is noted as such in the docs.

use std::path::PathBuf;
use std::sync::Arc;

use super::{
	CommandRunner, LaunchSpec, Registrar, RegistrationError, RegistrationState, first_line,
};
use crate::SERVICE_LABEL;

const LAUNCHCTL: &str = "/bin/launchctl";

pub struct MacosRegistrar {
	runner: Arc<dyn CommandRunner>,
	home: PathBuf,
	uid: String,
}

impl MacosRegistrar {
	pub fn new(runner: Arc<dyn CommandRunner>, home: PathBuf, uid: String) -> Self {
		Self { runner, home, uid }
	}

	pub fn for_current_user(runner: Arc<dyn CommandRunner>) -> Self {
		let home = std::env::var_os("HOME")
			.map(PathBuf::from)
			.unwrap_or_default();
		let uid = runner
			.run("/usr/bin/id", &["-u".into()])
			.ok()
			.filter(|output| output.success)
			.map(|output| output.stdout.trim().to_owned())
			.unwrap_or_default();

		Self::new(runner, home, uid)
	}

	pub fn plist_path(&self) -> PathBuf {
		self.home
			.join("Library/LaunchAgents")
			.join(format!("{SERVICE_LABEL}.plist"))
	}

	fn domain(&self) -> String {
		format!("gui/{}", self.uid)
	}

	fn target(&self) -> String {
		format!("{}/{SERVICE_LABEL}", self.domain())
	}

	fn launchctl(&self, args: &[&str]) -> Result<super::CommandOutput, RegistrationError> {
		let args: Vec<String> = args.iter().map(|arg| (*arg).to_owned()).collect();

		Ok(self.runner.run(LAUNCHCTL, &args)?)
	}
}

fn xml_escape(text: &str) -> String {
	text.replace('&', "&amp;")
		.replace('<', "&lt;")
		.replace('>', "&gt;")
		.replace('"', "&quot;")
}

/// The LaunchAgent plist for `spec`.
///
/// It runs once at login and is not kept alive: if the updater exits or
/// crashes, launchd leaves it stopped instead of restarting it.
pub fn launch_agent_plist(spec: &LaunchSpec) -> String {
	let binary = xml_escape(&spec.binary.to_string_lossy());
	let data = xml_escape(&spec.data_dir.to_string_lossy());
	let log = xml_escape(
		&spec
			.data_dir
			.join("background/launchd.log")
			.to_string_lossy(),
	);

	format!(
		r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>Label</key>
	<string>{SERVICE_LABEL}</string>
	<key>ProgramArguments</key>
	<array>
		<string>{binary}</string>
		<string>--data-dir</string>
		<string>{data}</string>
	</array>
	<key>RunAtLoad</key>
	<true/>
	<key>KeepAlive</key>
	<false/>
	<key>ProcessType</key>
	<string>Background</string>
	<key>LowPriorityBackgroundIO</key>
	<true/>
	<key>StandardOutPath</key>
	<string>{log}</string>
	<key>StandardErrorPath</key>
	<string>{log}</string>
</dict>
</plist>
"#
	)
}

impl Registrar for MacosRegistrar {
	fn mechanism(&self) -> &'static str {
		"per-user LaunchAgent"
	}

	fn register(&self, spec: &LaunchSpec) -> Result<(), RegistrationError> {
		if self.uid.is_empty() || self.home.as_os_str().is_empty() {
			return Err(RegistrationError::Unsupported(
				"the user's home folder or ID could not be found".into(),
			));
		}

		let plist = self.plist_path();

		if let Some(folder) = plist.parent() {
			std::fs::create_dir_all(folder)?;
		}

		if let Some(folder) = spec.data_dir.join("background").to_str().map(PathBuf::from) {
			std::fs::create_dir_all(folder)?;
		}

		std::fs::write(&plist, launch_agent_plist(spec))?;

		// A copy that is already loaded would make bootstrap fail.
		let _ = self.launchctl(&["bootout", &self.target()]);

		let loaded = self.launchctl(&["bootstrap", &self.domain(), &plist.to_string_lossy()])?;

		if loaded.success {
			return Ok(());
		}

		let reason = first_line(&format!("{} {}", loaded.stderr, loaded.stdout));

		// The file is in place, so it will load at the next login even though
		// loading it now was refused.
		if reason.to_lowercase().contains("not permitted")
			|| reason.to_lowercase().contains("approval")
		{
			return Ok(());
		}

		Err(RegistrationError::Refused(reason))
	}

	fn unregister(&self) -> Result<(), RegistrationError> {
		let _ = self.launchctl(&["bootout", &self.target()]);

		match std::fs::remove_file(self.plist_path()) {
			Ok(()) => Ok(()),
			Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
			Err(error) => Err(error.into()),
		}
	}

	fn state(&self) -> RegistrationState {
		if !self.plist_path().is_file() {
			return RegistrationState::NotInstalled;
		}

		// The user can switch a background item off in System Settings. That
		// shows up as a disabled override, and it is respected, not undone.
		let disabled = self.launchctl(&["print-disabled", &self.domain()]).ok();

		if let Some(output) = disabled
			&& output.stdout.lines().any(|line| {
				line.contains(SERVICE_LABEL)
					&& line.contains("disabled")
					&& !line.contains("enabled")
			}) {
			return RegistrationState::DisabledByOs;
		}

		RegistrationState::Registered
	}

	fn start(&self) -> Result<(), RegistrationError> {
		let output = self.launchctl(&["kickstart", &self.target()])?;

		if output.success {
			Ok(())
		} else {
			Err(RegistrationError::Refused(first_line(&output.stderr)))
		}
	}

	fn stop(&self) -> Result<(), RegistrationError> {
		let output = self.launchctl(&["kill", "SIGTERM", &self.target()])?;

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

	/// A spec whose data folder is real and disposable.
	fn spec_in(home: &std::path::Path) -> LaunchSpec {
		LaunchSpec {
			binary: spec().binary,
			data_dir: home.join("data"),
		}
	}

	fn registrar(home: &std::path::Path) -> (Arc<FakeRunner>, MacosRegistrar) {
		let runner = Arc::new(FakeRunner::default());

		(
			Arc::clone(&runner),
			MacosRegistrar::new(runner, home.to_path_buf(), "501".into()),
		)
	}

	#[test]
	fn the_plist_runs_once_at_login_and_is_not_kept_alive() {
		let plist = launch_agent_plist(&spec());

		assert!(plist.contains("<key>RunAtLoad</key>\n\t<true/>"));
		assert!(plist.contains("<key>KeepAlive</key>\n\t<false/>"));
		assert!(plist.contains("<string>/opt/remade/remade-launcher-daemon</string>"));
		assert!(plist.contains("<string>--data-dir</string>"));
		assert!(!plist.contains("UserName"), "no other user, and no root");
		assert!(plist.contains(SERVICE_LABEL));
	}

	#[test]
	fn paths_cannot_break_out_of_the_plist() {
		let mut tricky = spec();
		tricky.binary = PathBuf::from("/opt/<evil>&\"x\"/daemon");

		let plist = launch_agent_plist(&tricky);

		assert!(plist.contains("/opt/&lt;evil&gt;&amp;&quot;x&quot;/daemon"));
		assert!(!plist.contains("<evil>"));
	}

	#[test]
	fn registering_writes_the_plist_under_the_users_home_and_loads_it() {
		let home = tempfile::tempdir().unwrap();
		let (runner, registrar) = registrar(home.path());

		registrar.register(&spec_in(home.path())).unwrap();

		let plist = home
			.path()
			.join("Library/LaunchAgents")
			.join(format!("{SERVICE_LABEL}.plist"));
		assert!(plist.is_file());

		let calls = runner.calls().join("\n");
		assert!(calls.contains("bootstrap gui/501"), "{calls}");
		assert!(!calls.contains("sudo") && !calls.contains("/Library/LaunchDaemons"));
	}

	#[test]
	fn unregistering_unloads_and_removes_and_is_repeatable() {
		let home = tempfile::tempdir().unwrap();
		let (runner, registrar) = registrar(home.path());
		registrar.register(&spec_in(home.path())).unwrap();

		registrar.unregister().unwrap();
		registrar.unregister().unwrap();

		assert!(!registrar.plist_path().exists());
		assert!(
			runner
				.calls()
				.iter()
				.any(|call| call.contains("bootout gui/501/"))
		);
		assert_eq!(registrar.state(), RegistrationState::NotInstalled);
	}

	#[test]
	fn state_comes_from_the_system_and_respects_a_user_who_turned_it_off() {
		let home = tempfile::tempdir().unwrap();
		let (runner, registrar) = registrar(home.path());

		assert_eq!(registrar.state(), RegistrationState::NotInstalled);

		registrar.register(&spec_in(home.path())).unwrap();
		assert_eq!(registrar.state(), RegistrationState::Registered);

		runner.answer(
			"launchctl",
			"print-disabled",
			true,
			&format!("disabled services = {{\n\t\"{SERVICE_LABEL}\" => disabled\n}}"),
		);
		assert_eq!(registrar.state(), RegistrationState::DisabledByOs);
	}

	#[test]
	fn a_refusal_is_reported_with_one_line_of_the_reason() {
		let home = tempfile::tempdir().unwrap();
		let (runner, registrar) = registrar(home.path());
		runner.answer("launchctl", "bootstrap", false, "");
		runner.script.lock().unwrap()[0].2.stderr =
			"Bootstrap failed: 5: Input/output error\nmore detail with /private/path".into();

		let error = registrar.register(&spec_in(home.path())).unwrap_err();

		assert!(error.to_string().contains("Input/output error"));
		assert!(!error.to_string().contains("/private/path"));
	}

	#[test]
	fn without_a_user_id_it_refuses_instead_of_guessing() {
		let registrar = MacosRegistrar::new(
			Arc::new(FakeRunner::default()),
			PathBuf::from("/h"),
			String::new(),
		);

		assert!(matches!(
			registrar.register(&spec()),
			Err(RegistrationError::Unsupported(_))
		));
	}
}
