use std::path::PathBuf;
use std::process::{Command, Stdio};

use launcher_model::{ErrorCode, InstallationId, LauncherError};

use super::install::{LifecycleService, not_found};
use super::remove::verify_ownership;

impl LifecycleService {
	/// Starts an installed app.
	///
	/// Ownership is proven again first: the folder must still carry this
	/// installation's marker, and the entry point must resolve inside it.
	pub async fn launch(&self, installation: InstallationId) -> Result<(), LauncherError> {
		let record = self
			.database
			.installation(installation)
			.await?
			.ok_or_else(not_found)?;
		let apps = self.data.apps_dir();

		tokio::task::spawn_blocking(move || {
			let root = verify_ownership(&apps, &record)?;
			let entry = record.entry_point.as_deref().ok_or_else(|| {
				LauncherError::new(
					ErrorCode::LaunchFailed,
					"The launcher does not know which file starts this app.",
				)
			})?;

			if !launcher_packages::is_safe_relative_path(entry) {
				return Err(launch_failed());
			}

			let target = root
				.join(entry)
				.canonicalize()
				.map_err(|_| launch_failed())?;

			// A symlink at the entry point must not lead out of the app.
			if !target.starts_with(root.canonicalize().map_err(|_| launch_failed())?) {
				return Err(launch_failed());
			}

			spawn(target, &root)
		})
		.await
		.map_err(super::install::internal)?
	}
}

fn launch_failed() -> LauncherError {
	LauncherError::new(ErrorCode::LaunchFailed, "The app could not be started.")
}

fn spawn(target: PathBuf, root: &std::path::Path) -> Result<(), LauncherError> {
	let mut command =
		if cfg!(target_os = "macos") && target.extension().is_some_and(|ext| ext == "app") {
			let mut command = Command::new("/usr/bin/open");
			command.arg(&target);
			command
		} else {
			let mut command = Command::new(&target);
			command.current_dir(root);
			command
		};

	command
		.stdin(Stdio::null())
		.stdout(Stdio::null())
		.stderr(Stdio::null())
		.spawn()
		.map(drop)
		.map_err(|error| launch_failed().with_source(error))
}
