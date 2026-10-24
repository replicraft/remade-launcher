use std::path::{Path, PathBuf};

use directories::ProjectDirs;
use launcher_model::{ErrorCode, LauncherError};

/// Where the launcher keeps its own state.
#[derive(Clone, Debug)]
pub struct DataDirectory {
	root: PathBuf,
}

impl DataDirectory {
	/// The platform's per-user data directory for the launcher.
	pub fn platform_default() -> Result<Self, LauncherError> {
		let directories = ProjectDirs::from("wiki", "remade", "Launcher").ok_or_else(|| {
			LauncherError::new(
				ErrorCode::StorageUnavailable,
				"This system has no home directory for the launcher to use.",
			)
		})?;

		Ok(Self::at(directories.data_dir()))
	}

	/// A specific directory, mainly for tests.
	pub fn at(root: impl Into<PathBuf>) -> Self {
		Self { root: root.into() }
	}

	pub fn root(&self) -> &Path {
		&self.root
	}

	pub fn database_file(&self) -> PathBuf {
		self.root.join("launcher.sqlite")
	}

	/// Where installed apps live. Staging sits beside it on the same
	/// filesystem so finishing an install is one atomic rename.
	pub fn apps_dir(&self) -> PathBuf {
		self.root.join("apps")
	}

	/// Scratch space for in-flight operations, one folder per operation.
	pub fn staging_dir(&self) -> PathBuf {
		self.root.join("staging")
	}

	/// Creates the directory if it does not exist.
	pub fn ensure_exists(&self) -> Result<(), LauncherError> {
		std::fs::create_dir_all(&self.root).map_err(|error| {
			LauncherError::new(
				ErrorCode::StorageUnavailable,
				"The launcher's data folder could not be created.",
			)
			.with_source(error)
		})
	}
}
