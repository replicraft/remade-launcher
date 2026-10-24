//! Favorites: references to apps, kept to come back to.

use launcher_model::LauncherError;

use crate::Database;
use crate::error::{DbResult, unavailable};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Favorite {
	/// `catalog` or `repository`.
	pub kind: String,
	pub reference: String,
	pub added_at: i64,
}

impl Database {
	/// Adds a favorite. Adding one that is already there changes nothing.
	/// Returns whether it was new.
	pub async fn add_favorite(
		&self,
		kind: &'static str,
		reference: String,
		now: i64,
	) -> Result<bool, LauncherError> {
		self.writer()
			.call(move |connection| -> DbResult<bool> {
				Ok(connection.execute(
					"INSERT OR IGNORE INTO favorites (kind, reference, added_at) VALUES (?1, ?2, ?3)",
					(kind, reference, now),
				)? > 0)
			})
			.await
			.map_err(unavailable)
	}

	pub async fn favorites(&self) -> Result<Vec<Favorite>, LauncherError> {
		self.reader()
			.call(|connection| -> DbResult<_> {
				let mut statement = connection.prepare_cached(
					"SELECT kind, reference, added_at FROM favorites ORDER BY reference COLLATE NOCASE",
				)?;
				let rows = statement
					.query_map([], |row| {
						Ok(Favorite {
							kind: row.get(0)?,
							reference: row.get(1)?,
							added_at: row.get(2)?,
						})
					})?
					.collect::<Result<Vec<_>, _>>()?;

				Ok(rows)
			})
			.await
			.map_err(unavailable)
	}

	pub async fn remove_favorite(
		&self,
		kind: &'static str,
		reference: String,
	) -> Result<bool, LauncherError> {
		self.writer()
			.call(move |connection| -> DbResult<bool> {
				Ok(connection.execute(
					"DELETE FROM favorites WHERE kind = ?1 AND reference = ?2",
					(kind, reference),
				)? > 0)
			})
			.await
			.map_err(unavailable)
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[tokio::test]
	async fn favorites_are_added_once_listed_and_removed() {
		let directory = tempfile::tempdir().unwrap();
		let database = Database::open(directory.path().join("launcher.sqlite"))
			.await
			.unwrap();

		assert!(
			database
				.add_favorite("catalog", "quill".into(), 1)
				.await
				.unwrap()
		);
		assert!(
			!database
				.add_favorite("catalog", "quill".into(), 2)
				.await
				.unwrap(),
			"already there"
		);
		assert!(
			database
				.add_favorite("repository", "o/r".into(), 3)
				.await
				.unwrap()
		);

		let all = database.favorites().await.unwrap();

		assert_eq!(all.len(), 2);
		assert_eq!(
			all.iter()
				.find(|favorite| favorite.reference == "quill")
				.unwrap()
				.added_at,
			1,
			"the first time is kept"
		);
		assert!(
			database
				.remove_favorite("catalog", "quill".into())
				.await
				.unwrap()
		);
		assert!(
			!database
				.remove_favorite("catalog", "quill".into())
				.await
				.unwrap()
		);
	}
}
