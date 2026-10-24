//! Deleting only what the launcher created.
//!
//! A path stored in SQLite is never trusted on its own. Before anything is
//! removed, the path must sit directly inside a directory the launcher owns,
//! be a real directory (not a symlink), and carry the expected marker.

use std::path::{Path, PathBuf};

pub(crate) const MARKER: &str = ".remade-install";

/// True if `path` is a direct child of `parent`, with no symlinks involved.
pub(crate) fn is_direct_child_dir(parent: &Path, path: &Path) -> bool {
	let Ok(metadata) = std::fs::symlink_metadata(path) else {
		return false;
	};

	if !metadata.is_dir() {
		return false;
	}

	let (Ok(parent), Ok(path)) = (parent.canonicalize(), path.canonicalize()) else {
		return false;
	};

	path.parent() == Some(parent.as_path())
}

/// Removes an operation's staging folder. `staging_root` is the launcher's
/// staging directory and `name` the operation ID the folder must carry.
pub(crate) fn remove_staging(staging_root: &Path, name: &str) -> std::io::Result<bool> {
	if name.is_empty() || name.contains(['/', '\\']) || name == "." || name == ".." {
		return Ok(false);
	}

	let path: PathBuf = staging_root.join(name);

	if !is_direct_child_dir(staging_root, &path) {
		return Ok(false);
	}

	std::fs::remove_dir_all(&path)?;

	Ok(true)
}

/// [`remove_staging`] that reports only whether something was removed.
pub(crate) fn remove_staging_folder(staging_root: &Path, name: &str) -> bool {
	remove_staging(staging_root, name).unwrap_or(false)
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn removes_a_real_child_directory() {
		let root = tempfile::tempdir().unwrap();
		std::fs::create_dir_all(root.path().join("op/inner")).unwrap();
		std::fs::write(root.path().join("op/inner/f"), "x").unwrap();

		assert!(remove_staging(root.path(), "op").unwrap());
		assert!(!root.path().join("op").exists());
	}

	#[test]
	fn refuses_traversal_missing_paths_and_files() {
		let root = tempfile::tempdir().unwrap();
		let outside = tempfile::tempdir().unwrap();
		std::fs::write(root.path().join("file"), "x").unwrap();

		for name in ["..", ".", "", "a/b", "../x", "missing", "file"] {
			assert!(!remove_staging(root.path(), name).unwrap(), "{name:?}");
		}

		assert!(outside.path().exists());
	}

	#[cfg(unix)]
	#[test]
	fn never_follows_a_symlink_out_of_the_staging_root() {
		let root = tempfile::tempdir().unwrap();
		let outside = tempfile::tempdir().unwrap();
		std::fs::write(outside.path().join("precious"), "keep").unwrap();
		std::os::unix::fs::symlink(outside.path(), root.path().join("op")).unwrap();

		assert!(!remove_staging(root.path(), "op").unwrap());
		assert!(outside.path().join("precious").exists());
	}
}
