//! One writer of installed apps at a time, across processes.
//!
//! The desktop app and the command-line tool share one data directory. An
//! operating-system file lock makes sure only one of them installs or removes
//! at once, and lets startup recovery tell "crashed" from "still running":
//! recovery only touches the journal when it can take the lock itself.

use std::fs::{File, OpenOptions, TryLockError};

use launcher_model::{ErrorCode, LauncherError};

use crate::paths::DataDirectory;

/// Held for the length of one operation. Dropping it releases the lock.
#[derive(Debug)]
pub(crate) struct OperationLock {
	_file: File,
}

fn open(data: &DataDirectory) -> Result<File, LauncherError> {
	OpenOptions::new()
		.create(true)
		.truncate(false)
		.write(true)
		.open(data.root().join("operations.lock"))
		.map_err(|error| {
			LauncherError::new(
				ErrorCode::StorageUnavailable,
				"The launcher could not prepare its lock file.",
			)
			.with_source(error)
		})
}

/// Takes the lock, or reports that another install or removal is running.
pub(crate) fn acquire(data: &DataDirectory) -> Result<OperationLock, LauncherError> {
	let file = open(data)?;

	match file.try_lock() {
		Ok(()) => Ok(OperationLock { _file: file }),
		Err(TryLockError::WouldBlock) => Err(LauncherError::new(
			ErrorCode::OperationInProgress,
			"Another install or removal is already running. Try again when it finishes.",
		)),
		Err(TryLockError::Error(error)) => Err(LauncherError::new(
			ErrorCode::StorageUnavailable,
			"The launcher could not take its operation lock.",
		)
		.with_source(error)),
	}
}

/// Like [`acquire`], but `None` when busy. For recovery, which simply waits
/// for a later start.
pub(crate) fn try_acquire(data: &DataDirectory) -> Option<OperationLock> {
	acquire(data).ok()
}

#[cfg(test)]
mod tests {
	use super::*;

	fn data() -> (tempfile::TempDir, DataDirectory) {
		let directory = tempfile::tempdir().unwrap();
		let data = DataDirectory::at(directory.path());

		(directory, data)
	}

	#[test]
	fn a_second_holder_is_refused_until_the_first_lets_go() {
		let (_directory, data) = data();

		let first = acquire(&data).unwrap();
		let refused = acquire(&data).unwrap_err();

		assert_eq!(refused.code(), ErrorCode::OperationInProgress);
		assert!(try_acquire(&data).is_none());

		drop(first);

		assert!(acquire(&data).is_ok());
	}
}
