//! Apps the person asked the launcher to watch without owning.

use launcher_model::{InstallationId, LauncherError};
use rusqlite::OptionalExtension;

use crate::Database;
use crate::error::{DbResult, unavailable};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AdoptedRecord {
	pub id: InstallationId,
	pub display_name: String,
	pub path: String,
	pub entry_point: Option<String>,
	pub catalog_slug: Option<String>,
	pub adopted_at: i64,
	pub last_seen_at: Option<i64>,
}

fn read(row: &rusqlite::Row<'_>) -> rusqlite::Result<AdoptedRecord> {
	let id: Vec<u8> = row.get(0)?;

	Ok(AdoptedRecord {
		id: InstallationId::from_bytes(id.as_slice().try_into().unwrap_or([0; 16])),
		display_name: row.get(1)?,
		path: row.get(2)?,
		entry_point: row.get(3)?,
		catalog_slug: row.get(4)?,
		adopted_at: row.get(5)?,
		last_seen_at: row.get(6)?,
	})
}

const COLUMNS: &str = "id, display_name, path, entry_point, catalog_slug, adopted_at, last_seen_at";

impl Database {
	/// Records an adopted app. Fails if the folder is already recorded.
	pub async fn add_adopted(&self, record: AdoptedRecord) -> Result<(), LauncherError> {
		self.writer()
			.call(move |connection| -> DbResult<()> {
				connection
					.prepare_cached(
						"INSERT INTO adopted_apps (id, display_name, path, entry_point, catalog_slug, adopted_at, last_seen_at)
						 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
					)?
					.execute((
						record.id.as_bytes().as_slice(),
						record.display_name,
						record.path,
						record.entry_point,
						record.catalog_slug,
						record.adopted_at,
						record.last_seen_at,
					))?;

				Ok(())
			})
			.await
			.map_err(unavailable)
	}

	pub async fn adopted_apps(&self) -> Result<Vec<AdoptedRecord>, LauncherError> {
		self.reader()
			.call(|connection| -> DbResult<_> {
				let mut statement = connection.prepare_cached(&format!(
					"SELECT {COLUMNS} FROM adopted_apps ORDER BY display_name COLLATE NOCASE"
				))?;
				let rows = statement
					.query_map([], read)?
					.collect::<Result<Vec<_>, _>>()?;

				Ok(rows)
			})
			.await
			.map_err(unavailable)
	}

	pub async fn adopted_app(
		&self,
		id: InstallationId,
	) -> Result<Option<AdoptedRecord>, LauncherError> {
		self.reader()
			.call(move |connection| -> DbResult<_> {
				connection
					.prepare_cached(&format!("SELECT {COLUMNS} FROM adopted_apps WHERE id = ?1"))?
					.query_row([id.as_bytes().as_slice()], read)
					.optional()
			})
			.await
			.map_err(unavailable)
	}

	/// Drops the record. The files are not touched. Returns whether there
	/// was one.
	pub async fn remove_adopted(&self, id: InstallationId) -> Result<bool, LauncherError> {
		self.writer()
			.call(move |connection| -> DbResult<bool> {
				Ok(connection.execute(
					"DELETE FROM adopted_apps WHERE id = ?1",
					[id.as_bytes().as_slice()],
				)? > 0)
			})
			.await
			.map_err(unavailable)
	}

	pub async fn mark_adopted_seen(
		&self,
		id: InstallationId,
		now: i64,
	) -> Result<(), LauncherError> {
		self.writer()
			.call(move |connection| -> DbResult<()> {
				connection.execute(
					"UPDATE adopted_apps SET last_seen_at = ?2 WHERE id = ?1",
					(id.as_bytes().as_slice(), now),
				)?;

				Ok(())
			})
			.await
			.map_err(unavailable)
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	fn record(path: &str) -> AdoptedRecord {
		AdoptedRecord {
			id: InstallationId::new(),
			display_name: "Thing".into(),
			path: path.into(),
			entry_point: Some("thing".into()),
			catalog_slug: None,
			adopted_at: 1,
			last_seen_at: None,
		}
	}

	#[tokio::test]
	async fn an_adopted_app_round_trips_and_a_folder_can_only_be_adopted_once() {
		let directory = tempfile::tempdir().unwrap();
		let database = Database::open(directory.path().join("launcher.sqlite"))
			.await
			.unwrap();
		let first = record("/apps/thing");

		database.add_adopted(first.clone()).await.unwrap();

		assert_eq!(
			database.adopted_app(first.id).await.unwrap(),
			Some(first.clone())
		);
		assert_eq!(database.adopted_apps().await.unwrap(), vec![first.clone()]);
		assert!(
			database.add_adopted(record("/apps/thing")).await.is_err(),
			"the same folder twice"
		);

		database.mark_adopted_seen(first.id, 50).await.unwrap();

		assert_eq!(
			database
				.adopted_app(first.id)
				.await
				.unwrap()
				.unwrap()
				.last_seen_at,
			Some(50)
		);
		assert!(database.remove_adopted(first.id).await.unwrap());
		assert!(!database.remove_adopted(first.id).await.unwrap());
	}
}
