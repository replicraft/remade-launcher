//! Registering the updater to start at login, on each operating system.
//!
//! Every adapter works only for the current user and with ordinary
//! privileges: a login item, a user service, or a per-user scheduled task.
//! None installs a system service or asks for administrator rights. The
//! adapters talk to the operating system through a [`CommandRunner`], so
//! their behavior is tested with a fake one and no test touches a real
//! machine's login items.

mod linux;
mod macos;
mod windows;

use std::path::PathBuf;
use std::sync::Arc;

use thiserror::Error;

pub use linux::{LinuxRegistrar, systemd_quote, xdg_quote};
pub use macos::{MacosRegistrar, launch_agent_plist};
pub use windows::{WindowsRegistrar, task_command};

/// What to start at login.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LaunchSpec {
	/// The updater executable. An absolute path to a file the launcher
	/// shipped, never something found on `PATH`.
	pub binary: PathBuf,
	pub data_dir: PathBuf,
}

/// The truth about a registration, asked of the operating system rather than
/// echoed from a saved preference.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RegistrationState {
	NotInstalled,
	/// Registered and enabled. Whether it is running is a separate question.
	Registered,
	/// The system is waiting for the user to approve the login item.
	PendingApproval,
	/// The user turned it off in the system's own settings. The launcher
	/// never turns it back on by itself.
	DisabledByOs,
	Failed(String),
	/// This system has no per-user login mechanism to use, such as a
	/// headless machine with no session manager.
	Unsupported(String),
}

impl RegistrationState {
	/// A stable word for status output.
	pub fn code(&self) -> &'static str {
		match self {
			Self::NotInstalled => "not_installed",
			Self::Registered => "registered",
			Self::PendingApproval => "pending_os_approval",
			Self::DisabledByOs => "disabled_by_os",
			Self::Failed(_) => "failed",
			Self::Unsupported(_) => "unsupported",
		}
	}
}

#[derive(Debug, Error)]
pub enum RegistrationError {
	#[error("this system has no per-user login mechanism: {0}")]
	Unsupported(String),
	#[error("the operating system refused: {0}")]
	Refused(String),
	#[error("the login item could not be written: {0}")]
	Io(#[from] std::io::Error),
}

pub trait Registrar: Send + Sync {
	/// A short name for the mechanism, for status output.
	fn mechanism(&self) -> &'static str;

	/// Registers the updater to start at login, and (where the system does it
	/// as part of registering) starts it.
	fn register(&self, spec: &LaunchSpec) -> Result<(), RegistrationError>;

	fn unregister(&self) -> Result<(), RegistrationError>;

	fn state(&self) -> RegistrationState;

	/// Starts the registered updater now.
	fn start(&self) -> Result<(), RegistrationError>;

	/// Asks the running updater to stop. It may start again at next login.
	fn stop(&self) -> Result<(), RegistrationError>;
}

/// The result of running a system tool.
#[derive(Clone, Debug, Default)]
pub struct CommandOutput {
	pub success: bool,
	pub stdout: String,
	pub stderr: String,
}

/// How adapters run system tools. Programs are named by absolute path, with
/// fixed arguments, and nothing goes through a shell.
pub trait CommandRunner: Send + Sync {
	fn run(&self, program: &str, args: &[String]) -> std::io::Result<CommandOutput>;
}

pub struct SystemRunner;

impl CommandRunner for SystemRunner {
	fn run(&self, program: &str, args: &[String]) -> std::io::Result<CommandOutput> {
		let output = std::process::Command::new(program)
			.args(args)
			.stdin(std::process::Stdio::null())
			.output()?;

		Ok(CommandOutput {
			success: output.status.success(),
			stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
			stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
		})
	}
}

/// The registrar for the system the launcher is running on.
pub fn host_registrar() -> Box<dyn Registrar> {
	let runner: Arc<dyn CommandRunner> = Arc::new(SystemRunner);

	if cfg!(target_os = "macos") {
		Box::new(MacosRegistrar::for_current_user(runner))
	} else if cfg!(windows) {
		Box::new(WindowsRegistrar::new(runner))
	} else {
		Box::new(LinuxRegistrar::for_current_user(runner))
	}
}

/// A message safe to show: tool output can carry paths and names, so only
/// the first line is kept.
pub(crate) fn first_line(text: &str) -> String {
	text.lines()
		.find(|line| !line.trim().is_empty())
		.unwrap_or("no details")
		.trim()
		.chars()
		.take(200)
		.collect()
}

#[cfg(test)]
pub(crate) mod testing {
	use std::sync::Mutex;

	use super::*;

	/// Records every call and answers from a script.
	#[derive(Default)]
	pub(crate) struct FakeRunner {
		pub(crate) calls: Mutex<Vec<(String, Vec<String>)>>,
		/// `(program fragment, argument fragment, output)` in priority order.
		pub(crate) script: Mutex<Vec<(String, String, CommandOutput)>>,
	}

	impl FakeRunner {
		pub(crate) fn answer(&self, program: &str, argument: &str, success: bool, stdout: &str) {
			self.script.lock().unwrap().push((
				program.to_owned(),
				argument.to_owned(),
				CommandOutput {
					success,
					stdout: stdout.to_owned(),
					stderr: String::new(),
				},
			));
		}

		pub(crate) fn calls(&self) -> Vec<String> {
			self.calls
				.lock()
				.unwrap()
				.iter()
				.map(|(program, args)| format!("{program} {}", args.join(" ")))
				.collect()
		}
	}

	impl CommandRunner for FakeRunner {
		fn run(&self, program: &str, args: &[String]) -> std::io::Result<CommandOutput> {
			self.calls
				.lock()
				.unwrap()
				.push((program.to_owned(), args.to_vec()));

			let joined = args.join(" ");
			let script = self.script.lock().unwrap();

			Ok(script
				.iter()
				.find(|(p, a, _)| program.contains(p.as_str()) && joined.contains(a.as_str()))
				.map(|(_, _, output)| output.clone())
				.unwrap_or(CommandOutput {
					success: true,
					..CommandOutput::default()
				}))
		}
	}

	pub(crate) fn spec() -> LaunchSpec {
		LaunchSpec {
			binary: PathBuf::from("/opt/remade/remade-launcher-daemon"),
			data_dir: PathBuf::from("/home/me/.local/share/Launcher"),
		}
	}
}
