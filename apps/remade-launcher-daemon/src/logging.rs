//! The updater's log file.
//!
//! It is plain text under the launcher's data folder, kept small by
//! rotating once at startup, and it records what the updater did without
//! tokens or private URLs: those never reach the engine's log lines.

use std::fs::{File, OpenOptions};
use std::sync::Mutex;

use launcher_background::BackgroundPaths;
use tracing_subscriber::EnvFilter;

/// A log larger than this is moved aside when the updater starts.
const ROTATE_AT_BYTES: u64 = 1024 * 1024;

pub(crate) fn start(paths: &BackgroundPaths) -> Result<(), String> {
	std::fs::create_dir_all(paths.folder())
		.map_err(|error| format!("could not create the updater's folder: {error}"))?;

	if std::fs::metadata(&paths.log).is_ok_and(|meta| meta.len() > ROTATE_AT_BYTES) {
		let _ = std::fs::rename(&paths.log, paths.log.with_extension("log.1"));
	}

	let file: File = OpenOptions::new()
		.create(true)
		.append(true)
		.open(&paths.log)
		.map_err(|error| format!("could not open the updater's log: {error}"))?;

	tracing_subscriber::fmt()
		.with_writer(Mutex::new(file))
		.with_ansi(false)
		.with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
		.init();

	Ok(())
}
