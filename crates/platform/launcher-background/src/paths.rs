use std::path::{Path, PathBuf};

use crate::DAEMON_BINARY;

/// Where the updater keeps its files, all under the launcher's data folder.
#[derive(Clone, Debug)]
pub struct BackgroundPaths {
	pub socket: PathBuf,
	pub log: PathBuf,
	pub lock: PathBuf,
}

impl BackgroundPaths {
	pub fn new(data_root: &Path) -> Self {
		let folder = data_root.join("background");

		Self {
			socket: folder.join("updater.sock"),
			log: folder.join("updater.log"),
			lock: folder.join("updater.lock"),
		}
	}

	pub fn folder(&self) -> &Path {
		self.socket.parent().unwrap_or(Path::new("."))
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
	fn everything_lives_under_one_private_folder() {
		let paths = BackgroundPaths::new(Path::new("/data/remade"));

		assert_eq!(paths.folder(), Path::new("/data/remade/background"));
		assert!(paths.socket.starts_with(paths.folder()));
		assert!(paths.log.starts_with(paths.folder()));
		assert!(paths.lock.starts_with(paths.folder()));
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
