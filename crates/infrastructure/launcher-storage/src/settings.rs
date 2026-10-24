//! Key/value settings.

use launcher_model::LauncherError;
use rusqlite::OptionalExtension;

use crate::Database;
use crate::error::{DbResult, unavailable};

impl Database {
	/// Reads a setting, or `None` if it was never saved.
	pub async fn setting(&self, key: &'static str) -> Result<Option<String>, LauncherError> {
		self.reader()
			.call(move |connection| -> DbResult<_> {
				let value = connection
					.prepare_cached("SELECT value FROM settings WHERE key = ?1")?
					.query_row([key], |row| row.get(0))
					.optional()?;

				Ok(value)
			})
			.await
			.map_err(unavailable)
	}

	/// Sets `key` to `value` only if it holds something else, and says whether
	/// it changed. Two processes racing to claim the same value cannot both
	/// win, which is how "once per launch" is enforced.
	pub async fn claim_setting(
		&self,
		key: &'static str,
		value: String,
	) -> Result<bool, LauncherError> {
		self.writer()
			.call(move |connection| -> DbResult<bool> {
				let changed = connection
					.prepare_cached(
						"INSERT INTO settings (key, value) VALUES (?1, ?2)
						 ON CONFLICT (key) DO UPDATE SET value = excluded.value
						 WHERE settings.value != excluded.value",
					)?
					.execute((key, value))?;

				Ok(changed > 0)
			})
			.await
			.map_err(unavailable)
	}

	/// Saves a setting, replacing any previous value.
	pub async fn set_setting(&self, key: &'static str, value: String) -> Result<(), LauncherError> {
		self.writer()
			.call(move |connection| -> DbResult<()> {
				connection
					.prepare_cached(
						"INSERT INTO settings (key, value) VALUES (?1, ?2)
						 ON CONFLICT (key) DO UPDATE SET value = excluded.value",
					)?
					.execute((key, value))?;

				Ok(())
			})
			.await
			.map_err(unavailable)
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	async fn open_temporary() -> (tempfile::TempDir, Database) {
		let directory = tempfile::tempdir().unwrap();
		let database = Database::open(directory.path().join("launcher.sqlite"))
			.await
			.unwrap();

		(directory, database)
	}

	#[tokio::test]
	async fn missing_setting_reads_as_none() {
		let (_directory, database) = open_temporary().await;

		assert_eq!(database.setting("theme").await.unwrap(), None);
	}

	#[tokio::test]
	async fn saved_setting_is_readable_and_replaceable() {
		let (_directory, database) = open_temporary().await;

		database.set_setting("theme", "dark".into()).await.unwrap();
		assert_eq!(
			database.setting("theme").await.unwrap().as_deref(),
			Some("dark")
		);

		database.set_setting("theme", "light".into()).await.unwrap();
		assert_eq!(
			database.setting("theme").await.unwrap().as_deref(),
			Some("light")
		);
	}

	#[tokio::test]
	async fn settings_survive_reopening() {
		let directory = tempfile::tempdir().unwrap();
		let path = directory.path().join("launcher.sqlite");

		let first = Database::open(&path).await.unwrap();
		first.set_setting("theme", "dark".into()).await.unwrap();
		first.close().await;

		let second = Database::open(&path).await.unwrap();
		assert_eq!(
			second.setting("theme").await.unwrap().as_deref(),
			Some("dark")
		);
	}

	#[tokio::test]
	async fn a_value_can_be_claimed_only_once() {
		let (_directory, database) = open_temporary().await;

		assert!(database.claim_setting("session", "a".into()).await.unwrap());
		assert!(!database.claim_setting("session", "a".into()).await.unwrap());
		assert!(database.claim_setting("session", "b".into()).await.unwrap());
		assert_eq!(
			database.setting("session").await.unwrap().as_deref(),
			Some("b")
		);
	}

	#[tokio::test]
	async fn readers_cannot_write() {
		let (_directory, database) = open_temporary().await;

		let attempt = database
			.reader()
			.call(|connection| -> DbResult<()> {
				connection.execute("INSERT INTO settings (key, value) VALUES ('a', 'b')", ())?;
				Ok(())
			})
			.await;

		assert!(attempt.is_err());
	}

	#[tokio::test]
	async fn reader_sees_committed_writes_across_connections() {
		let (_directory, database) = open_temporary().await;

		for index in 0..8 {
			let value = index.to_string();
			database
				.set_setting("counter", value.clone())
				.await
				.unwrap();

			// Round-robin touches every reader across the loop.
			assert_eq!(database.setting("counter").await.unwrap(), Some(value));
		}
	}
}
