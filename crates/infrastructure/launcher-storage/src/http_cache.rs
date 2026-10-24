//! Remembered API answers, kept so lookups can be revalidated cheaply.

use launcher_model::LauncherError;
use rusqlite::OptionalExtension;

use crate::Database;
use crate::error::{DbResult, unavailable};

/// How many answers are kept. The oldest go first.
const KEEP: i64 = 600;

/// One remembered answer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CachedResponse {
	pub etag: String,
	pub body: Vec<u8>,
}

impl Database {
	pub async fn cached_response(
		&self,
		url: String,
	) -> Result<Option<CachedResponse>, LauncherError> {
		self.reader()
			.call(move |connection| -> DbResult<_> {
				connection
					.prepare_cached("SELECT etag, body FROM http_cache WHERE url = ?1")?
					.query_row([url], |row| {
						Ok(CachedResponse {
							etag: row.get(0)?,
							body: row.get(1)?,
						})
					})
					.optional()
			})
			.await
			.map_err(unavailable)
	}

	/// Remembers an answer, replacing the one for the same address, and drops
	/// the oldest beyond the limit.
	pub async fn store_response(
		&self,
		url: String,
		etag: String,
		body: Vec<u8>,
		now: i64,
	) -> Result<(), LauncherError> {
		self.writer()
			.call(move |connection| -> DbResult<()> {
				connection
					.prepare_cached(
						"INSERT INTO http_cache (url, etag, body, stored_at) VALUES (?1, ?2, ?3, ?4)
						 ON CONFLICT (url) DO UPDATE SET
						     etag = excluded.etag, body = excluded.body, stored_at = excluded.stored_at",
					)?
					.execute((url, etag, body, now))?;
				connection
					.prepare_cached(
						"DELETE FROM http_cache WHERE url IN
						     (SELECT url FROM http_cache ORDER BY stored_at DESC LIMIT -1 OFFSET ?1)",
					)?
					.execute([KEEP])?;

				Ok(())
			})
			.await
			.map_err(unavailable)
	}

	/// Forgets every remembered answer. Returns how many there were.
	pub async fn clear_responses(&self) -> Result<usize, LauncherError> {
		self.writer()
			.call(|connection| -> DbResult<usize> {
				connection.execute("DELETE FROM http_cache", [])
			})
			.await
			.map_err(unavailable)
	}

	/// How many answers are remembered, and how many bytes they hold.
	pub async fn response_cache_size(&self) -> Result<(usize, u64), LauncherError> {
		self.reader()
			.call(|connection| -> DbResult<_> {
				let (count, bytes): (i64, i64) = connection.query_row(
					"SELECT count(*), coalesce(sum(length(body)), 0) FROM http_cache",
					[],
					|row| Ok((row.get(0)?, row.get(1)?)),
				)?;

				Ok((
					usize::try_from(count).unwrap_or(0),
					u64::try_from(bytes).unwrap_or(0),
				))
			})
			.await
			.map_err(unavailable)
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	async fn open() -> (tempfile::TempDir, Database) {
		let directory = tempfile::tempdir().unwrap();
		let database = Database::open(directory.path().join("launcher.sqlite"))
			.await
			.unwrap();

		(directory, database)
	}

	#[tokio::test]
	async fn an_answer_is_remembered_and_replaced() {
		let (_directory, database) = open().await;
		let url = "https://api.github.com/repos/o/r/releases".to_owned();

		assert_eq!(database.cached_response(url.clone()).await.unwrap(), None);

		database
			.store_response(url.clone(), "\"a\"".into(), b"one".to_vec(), 1)
			.await
			.unwrap();
		database
			.store_response(url.clone(), "\"b\"".into(), b"two".to_vec(), 2)
			.await
			.unwrap();

		assert_eq!(
			database.cached_response(url).await.unwrap(),
			Some(CachedResponse {
				etag: "\"b\"".into(),
				body: b"two".to_vec()
			})
		);
	}

	#[tokio::test]
	async fn only_the_newest_answers_are_kept() {
		let (_directory, database) = open().await;

		for index in 0..(KEEP + 25) {
			database
				.store_response(format!("https://x/{index}"), "e".into(), vec![0], index)
				.await
				.unwrap();
		}

		let (count, _) = database.response_cache_size().await.unwrap();

		assert_eq!(count, KEEP as usize);
		assert!(
			database
				.cached_response("https://x/0".into())
				.await
				.unwrap()
				.is_none(),
			"the oldest went first"
		);
		assert!(
			database
				.cached_response(format!("https://x/{}", KEEP + 24))
				.await
				.unwrap()
				.is_some()
		);
	}

	#[tokio::test]
	async fn clearing_forgets_everything() {
		let (_directory, database) = open().await;

		database
			.store_response("https://x/a".into(), "e".into(), vec![1, 2, 3], 1)
			.await
			.unwrap();

		assert_eq!(database.response_cache_size().await.unwrap(), (1, 3));
		assert_eq!(database.clear_responses().await.unwrap(), 1);
		assert_eq!(database.response_cache_size().await.unwrap(), (0, 0));
	}
}
