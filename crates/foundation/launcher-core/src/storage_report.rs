//! How much space the launcher uses, and clearing what is safe to clear.

use std::path::{Path, PathBuf};

use launcher_model::LauncherError;
use rayon::prelude::*;

use crate::engine::Engine;
use crate::lifecycle::{acquire_operation_lock, remove_staging_folder};

/// Where the launcher keeps things, and how big each part is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StorageReport {
	pub data_folder: PathBuf,
	pub apps_folder: PathBuf,
	pub apps_bytes: u64,
	/// Downloads and unpacked files from installs that are not running.
	pub leftover_bytes: u64,
	/// Remembered GitHub answers.
	pub cache_bytes: u64,
	pub database_bytes: u64,
}

/// The total size of everything under `path`, without following links.
/// Folders are walked in parallel.
fn folder_size(path: &Path) -> u64 {
	let Ok(entries) = std::fs::read_dir(path) else {
		return 0;
	};

	entries
		.flatten()
		.collect::<Vec<_>>()
		.par_iter()
		.map(|entry| match entry.file_type() {
			Ok(kind) if kind.is_dir() => folder_size(&entry.path()),
			Ok(kind) if kind.is_file() => entry.metadata().map_or(0, |metadata| metadata.len()),
			_ => 0,
		})
		.sum()
}

impl Engine {
	/// Measures the launcher's storage. This reads the disk, so it runs off
	/// the interface's thread.
	pub async fn storage_report(&self) -> Result<StorageReport, LauncherError> {
		let data = self.data.clone();
		let (_, cache_bytes) = self.database.response_cache_size().await?;

		let (apps_bytes, leftover_bytes, database_bytes) = tokio::task::spawn_blocking(move || {
			(
				folder_size(&data.apps_dir()),
				folder_size(&data.staging_dir()),
				std::fs::metadata(data.database_file()).map_or(0, |metadata| metadata.len()),
			)
		})
		.await
		.unwrap_or_default();

		Ok(StorageReport {
			data_folder: self.data.root().to_path_buf(),
			apps_folder: self.data.apps_dir(),
			apps_bytes,
			leftover_bytes,
			cache_bytes,
			database_bytes,
		})
	}

	/// Removes downloads and unpacked files left behind by installs that are
	/// not running and are not waiting for recovery. Returns the bytes freed.
	///
	/// This takes the same lock as an install, so it never touches one in
	/// progress, and it leaves alone anything the journal says an unfinished
	/// operation still needs.
	pub async fn clear_leftover_downloads(&self) -> Result<u64, LauncherError> {
		let _lock = acquire_operation_lock(&self.data).await?;
		let keep: Vec<String> = self
			.database
			.unfinished_operations()
			.await?
			.into_iter()
			.map(|record| record.id.to_string())
			.collect();
		let staging = self.data.staging_dir();

		Ok(tokio::task::spawn_blocking(move || {
			let Ok(entries) = std::fs::read_dir(&staging) else {
				return 0;
			};

			let mut freed = 0;

			for entry in entries.flatten() {
				let name = entry.file_name().to_string_lossy().into_owned();

				if keep.contains(&name) {
					continue;
				}

				let size = folder_size(&entry.path());

				if remove_staging_folder(&staging, &name) {
					freed += size;
				}
			}

			freed
		})
		.await
		.unwrap_or(0))
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::DataDirectory;

	#[test]
	fn folder_size_adds_up_nested_files_and_ignores_missing_folders() {
		let root = tempfile::tempdir().unwrap();

		std::fs::create_dir_all(root.path().join("a/b")).unwrap();
		std::fs::write(root.path().join("a/one"), [0u8; 100]).unwrap();
		std::fs::write(root.path().join("a/b/two"), [0u8; 50]).unwrap();

		assert_eq!(folder_size(root.path()), 150);
		assert_eq!(folder_size(&root.path().join("missing")), 0);
	}

	#[tokio::test]
	async fn leftovers_are_measured_then_cleared_and_the_apps_folder_is_untouched() {
		let directory = tempfile::tempdir().unwrap();
		let data = DataDirectory::at(directory.path().join("data"));
		let engine = Engine::open(&data).await.unwrap();

		std::fs::create_dir_all(data.staging_dir().join("old-op")).unwrap();
		std::fs::write(data.staging_dir().join("old-op/artifact"), [0u8; 4096]).unwrap();
		std::fs::create_dir_all(data.apps_dir().join("keep-me")).unwrap();
		std::fs::write(data.apps_dir().join("keep-me/file"), [0u8; 10]).unwrap();

		let before = engine.storage_report().await.unwrap();

		assert_eq!(before.leftover_bytes, 4096);
		assert_eq!(before.apps_bytes, 10);

		assert_eq!(engine.clear_leftover_downloads().await.unwrap(), 4096);

		let after = engine.storage_report().await.unwrap();

		assert_eq!(after.leftover_bytes, 0);
		assert_eq!(after.apps_bytes, 10, "installed apps are never touched");
	}
}
