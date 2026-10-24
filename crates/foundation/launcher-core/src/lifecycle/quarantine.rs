//! macOS Gatekeeper quarantine.
//!
//! Files downloaded by browsers carry a `com.apple.quarantine` attribute, and
//! that is what makes macOS check an app's signature and notarization on
//! first launch. The launcher downloads and unpacks the app itself, so it
//! must set the attribute itself. Skipping it would quietly bypass a
//! protection the user relies on.

use std::path::Path;

use launcher_model::{ErrorCode, LauncherError};

/// The quarantine record: flags, hex timestamp, and the downloading agent.
fn value(now: i64) -> String {
	format!("0083;{now:x};RemadeLauncher;")
}

/// Marks `tree` as downloaded from the internet. A no-op on other systems.
#[cfg(target_os = "macos")]
pub(crate) fn mark_downloaded(tree: &Path, now: i64) -> Result<(), LauncherError> {
	// `/usr/bin/xattr` is addressed by absolute path, with fixed arguments:
	// nothing from the catalog or the archive reaches the command line.
	let status = std::process::Command::new("/usr/bin/xattr")
		.args(["-r", "-w", "com.apple.quarantine", &value(now)])
		.arg(tree)
		.stdin(std::process::Stdio::null())
		.stdout(std::process::Stdio::null())
		.stderr(std::process::Stdio::null())
		.status();

	match status {
		Ok(status) if status.success() => Ok(()),
		_ => Err(LauncherError::new(
			ErrorCode::PackageUnsafe,
			"The launcher could not mark the download as coming from the internet, so it was not installed.",
		)),
	}
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn mark_downloaded(_tree: &Path, _now: i64) -> Result<(), LauncherError> {
	Ok(())
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
	use super::*;

	fn attribute(path: &Path) -> Option<String> {
		let output = std::process::Command::new("/usr/bin/xattr")
			.args(["-p", "com.apple.quarantine"])
			.arg(path)
			.output()
			.ok()?;

		output
			.status
			.success()
			.then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
	}

	#[test]
	fn marks_every_file_in_the_tree() {
		let directory = tempfile::tempdir().unwrap();
		std::fs::create_dir_all(directory.path().join("Tool.app/Contents")).unwrap();
		std::fs::write(directory.path().join("Tool.app/Contents/file"), "x").unwrap();

		mark_downloaded(directory.path(), 0x65a0_0000).unwrap();

		let expected = "0083;65a00000;RemadeLauncher;";

		assert_eq!(
			attribute(&directory.path().join("Tool.app")).as_deref(),
			Some(expected)
		);
		assert_eq!(
			attribute(&directory.path().join("Tool.app/Contents/file")).as_deref(),
			Some(expected)
		);
	}
}
