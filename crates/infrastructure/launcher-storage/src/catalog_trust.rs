//! Durable catalog trust state.
//!
//! The accepted manifest, its signature envelope, and the revision high-water
//! mark live in one row. A crash can therefore never leave a high-water mark
//! that points at missing bytes, or bytes that were never accepted.

use launcher_model::{ErrorCode, LauncherError};
use rusqlite::OptionalExtension;

use crate::Database;
use crate::error::{DbResult, unavailable};

/// What was accepted for one feed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StoredCatalog {
	pub feed_id: String,
	pub revision: u64,
	pub digest: [u8; 32],
	pub manifest: Vec<u8>,
	pub envelope: Vec<u8>,
	/// Unix seconds when this revision was accepted.
	pub accepted_at: i64,
	/// Latest trusted time at which any check ran. Used to notice a clock
	/// that jumped backwards.
	pub last_seen_at: i64,
}

impl Database {
	/// The accepted catalog for `feed_id`, if any.
	pub async fn catalog_trust(
		&self,
		feed_id: &str,
	) -> Result<Option<StoredCatalog>, LauncherError> {
		let feed_id = feed_id.to_owned();

		self.reader()
			.call(move |connection| -> DbResult<_> {
				connection
					.prepare_cached(
						"SELECT feed_id, revision, digest, manifest, envelope, accepted_at, last_seen_at
						 FROM catalog_trust WHERE feed_id = ?1",
					)?
					.query_row([feed_id], |row| {
						let digest: Vec<u8> = row.get(2)?;

						Ok(StoredCatalog {
							feed_id: row.get(0)?,
							revision: u64::try_from(row.get::<_, i64>(1)?)
								.map_err(|_| rusqlite::Error::InvalidQuery)?,
							digest: digest.try_into().map_err(|_| rusqlite::Error::InvalidQuery)?,
							manifest: row.get(3)?,
							envelope: row.get(4)?,
							accepted_at: row.get(5)?,
							last_seen_at: row.get(6)?,
						})
					})
					.optional()
			})
			.await
			.map_err(unavailable)
	}

	/// Records a newly verified catalog.
	///
	/// The previous snapshot stays in place until this transaction commits,
	/// and the revision can never move backwards, even if a caller forgets to
	/// check: the write is refused in SQL.
	pub async fn accept_catalog(&self, catalog: StoredCatalog) -> Result<(), LauncherError> {
		let changed = self
			.writer()
			.call(move |connection| -> DbResult<usize> {
				connection
					.prepare_cached(
						"INSERT INTO catalog_trust
						     (feed_id, revision, digest, manifest, envelope, accepted_at, last_seen_at)
						 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
						 ON CONFLICT (feed_id) DO UPDATE SET
						     revision     = excluded.revision,
						     digest       = excluded.digest,
						     manifest     = excluded.manifest,
						     envelope     = excluded.envelope,
						     accepted_at  = excluded.accepted_at,
						     last_seen_at = max(last_seen_at, excluded.last_seen_at)
						 WHERE excluded.revision > catalog_trust.revision",
					)?
					.execute((
						catalog.feed_id,
						// Revisions are capped at 2^53, so this cannot fail.
						i64::try_from(catalog.revision).unwrap_or(i64::MAX),
						catalog.digest.as_slice(),
						catalog.manifest,
						catalog.envelope,
						catalog.accepted_at,
						catalog.last_seen_at,
					))
			})
			.await
			.map_err(unavailable)?;

		if changed == 0 {
			return Err(LauncherError::new(
				ErrorCode::InternalInvariant,
				"The launcher refused to store an older catalog revision.",
			));
		}

		Ok(())
	}

	/// Moves `last_seen_at` forward without changing the accepted catalog.
	/// It never moves backwards.
	pub async fn note_catalog_check(&self, feed_id: &str, now: i64) -> Result<(), LauncherError> {
		let feed_id = feed_id.to_owned();

		self.writer()
			.call(move |connection| -> DbResult<()> {
				connection
					.prepare_cached(
						"UPDATE catalog_trust SET last_seen_at = max(last_seen_at, ?2) WHERE feed_id = ?1",
					)?
					.execute((feed_id, now))?;

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

	fn catalog(revision: u64) -> StoredCatalog {
		StoredCatalog {
			feed_id: "feed".into(),
			revision,
			digest: [revision as u8; 32],
			manifest: vec![1, 2, 3],
			envelope: vec![4, 5],
			accepted_at: 1_000,
			last_seen_at: 1_000,
		}
	}

	#[tokio::test]
	async fn nothing_is_stored_at_first() {
		let (_directory, database) = open_temporary().await;

		assert_eq!(database.catalog_trust("feed").await.unwrap(), None);
	}

	#[tokio::test]
	async fn an_accepted_catalog_round_trips() {
		let (_directory, database) = open_temporary().await;

		database.accept_catalog(catalog(5)).await.unwrap();

		assert_eq!(
			database.catalog_trust("feed").await.unwrap(),
			Some(catalog(5))
		);
	}

	#[tokio::test]
	async fn a_newer_revision_replaces_the_old_one() {
		let (_directory, database) = open_temporary().await;

		database.accept_catalog(catalog(5)).await.unwrap();
		database.accept_catalog(catalog(6)).await.unwrap();

		assert_eq!(
			database
				.catalog_trust("feed")
				.await
				.unwrap()
				.unwrap()
				.revision,
			6
		);
	}

	#[tokio::test]
	async fn the_high_water_mark_cannot_move_backwards() {
		let (_directory, database) = open_temporary().await;

		database.accept_catalog(catalog(6)).await.unwrap();

		assert!(database.accept_catalog(catalog(5)).await.is_err());
		assert!(database.accept_catalog(catalog(6)).await.is_err());
		assert_eq!(
			database.catalog_trust("feed").await.unwrap(),
			Some(catalog(6))
		);
	}

	#[tokio::test]
	async fn last_seen_only_moves_forward() {
		let (_directory, database) = open_temporary().await;
		database.accept_catalog(catalog(1)).await.unwrap();

		database.note_catalog_check("feed", 5_000).await.unwrap();
		database.note_catalog_check("feed", 2_000).await.unwrap();

		assert_eq!(
			database
				.catalog_trust("feed")
				.await
				.unwrap()
				.unwrap()
				.last_seen_at,
			5_000
		);
	}

	#[tokio::test]
	async fn feeds_are_independent() {
		let (_directory, database) = open_temporary().await;
		let mut other = catalog(1);
		other.feed_id = "other".into();

		database.accept_catalog(catalog(9)).await.unwrap();
		database.accept_catalog(other).await.unwrap();

		assert_eq!(
			database
				.catalog_trust("other")
				.await
				.unwrap()
				.unwrap()
				.revision,
			1
		);
	}
}
