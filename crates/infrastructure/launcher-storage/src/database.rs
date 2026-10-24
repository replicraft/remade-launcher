use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};

use launcher_model::LauncherError;
use rusqlite::OpenFlags;
use tokio_rusqlite::Connection;

use crate::error::{DbResult, unavailable};
use crate::{migrations, pragmas};

/// Upper bound on read-only connections. Each one costs a thread and a page
/// cache, and SQLite readers stop scaling well before this.
const MAX_READERS: usize = 4;

/// An open launcher database.
///
/// Cloning is not provided on purpose. Wrap it in an `Arc` to share it.
pub struct Database {
	writer: Connection,
	readers: Vec<Connection>,
	next_reader: AtomicUsize,
}

impl Database {
	/// Opens the database at `path`, creating and migrating it if needed.
	pub async fn open(path: impl AsRef<Path>) -> Result<Self, LauncherError> {
		let path = path.as_ref();

		let writer = Connection::open(path).await.map_err(unavailable)?;
		writer
			.call(|connection| -> DbResult<()> {
				pragmas::apply_writer(connection)?;
				Ok(())
			})
			.await
			.map_err(unavailable)?;

		// The migration error type is ours, so it travels out of the closure
		// as a value instead of through the rusqlite error channel.
		writer
			.call(|connection| -> DbResult<_> { Ok(migrations::run(connection)) })
			.await
			.map_err(unavailable)??;

		// Readers open after the writer so WAL mode and the schema exist.
		let reader_count = std::thread::available_parallelism()
			.map_or(2, |count| count.get())
			.clamp(1, MAX_READERS);

		let mut readers = Vec::with_capacity(reader_count);
		for _ in 0..reader_count {
			let reader = Connection::open_with_flags(
				path,
				OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
			)
			.await
			.map_err(unavailable)?;

			reader
				.call(|connection| -> DbResult<()> {
					pragmas::apply_reader(connection)?;
					Ok(())
				})
				.await
				.map_err(unavailable)?;

			readers.push(reader);
		}

		Ok(Self {
			writer,
			readers,
			next_reader: AtomicUsize::new(0),
		})
	}

	/// The connection that serializes all writes.
	pub(crate) fn writer(&self) -> &Connection {
		&self.writer
	}

	/// A read-only connection, chosen round-robin.
	pub(crate) fn reader(&self) -> &Connection {
		let index = self.next_reader.fetch_add(1, Ordering::Relaxed) % self.readers.len();

		&self.readers[index]
	}

	/// Lets SQLite refresh its query-planner statistics, then closes every
	/// connection. Call this on shutdown.
	pub async fn close(self) {
		let optimize = self
			.writer
			.call(|connection| -> DbResult<()> {
				connection.execute_batch("PRAGMA optimize;")?;
				Ok(())
			})
			.await;

		if let Err(error) = optimize {
			tracing::warn!(%error, "database optimize failed during shutdown");
		}

		for connection in self.readers.into_iter().chain([self.writer]) {
			if let Err(error) = connection.close().await {
				tracing::warn!(%error, "database connection did not close cleanly");
			}
		}
	}
}
