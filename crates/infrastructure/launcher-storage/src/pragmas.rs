//! Connection tuning.
//!
//! The values below favor read throughput and low latency, while keeping the
//! write path durable. The journal will hold records that protect user data
//! during destructive operations, so the writer syncs on every commit.

use rusqlite::Connection;

/// Memory-mapped I/O window. Kept small on 32-bit targets, where address
/// space is scarce.
#[cfg(target_pointer_width = "64")]
const MMAP_BYTES: u64 = 256 * 1024 * 1024;
#[cfg(not(target_pointer_width = "64"))]
const MMAP_BYTES: u64 = 16 * 1024 * 1024;

/// Applied to every connection.
fn apply_common(connection: &Connection) -> rusqlite::Result<()> {
	connection.busy_timeout(std::time::Duration::from_secs(5))?;

	connection.execute_batch(&format!(
		"PRAGMA foreign_keys = ON;
		 PRAGMA temp_store = MEMORY;
		 PRAGMA mmap_size = {MMAP_BYTES};"
	))
}

/// Applied to the writer. Switching to WAL persists in the database file, so
/// the readers inherit it.
pub(crate) fn apply_writer(connection: &Connection) -> rusqlite::Result<()> {
	apply_common(connection)?;

	// `journal_mode` returns a row, so it cannot go through `execute_batch`.
	connection.pragma_update(None, "journal_mode", "WAL")?;

	connection.execute_batch(
		"PRAGMA synchronous = FULL;
		 PRAGMA cache_size = -16384;
		 PRAGMA journal_size_limit = 67108864;
		 PRAGMA optimize = 0x10002;",
	)
}

/// Applied to read-only connections.
pub(crate) fn apply_reader(connection: &Connection) -> rusqlite::Result<()> {
	apply_common(connection)?;

	connection.execute_batch(
		"PRAGMA query_only = ON;
		 PRAGMA cache_size = -4096;
		 PRAGMA optimize = 0x10002;",
	)
}
