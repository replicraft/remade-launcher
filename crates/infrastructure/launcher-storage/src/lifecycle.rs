//! Receipts and the operation journal.

use launcher_model::{ErrorCode, InstallationId, LauncherError, OperationId};
use rusqlite::{OptionalExtension, Row};

use crate::Database;
use crate::error::{DbResult, unavailable};

/// One installed app. Plain strings and numbers: the core crate owns their
/// meaning, this crate only stores them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InstallationRecord {
	pub id: InstallationId,
	pub origin: String,
	pub catalog_slug: Option<String>,
	pub display_name: String,
	pub repository: String,
	pub repository_id: u64,
	pub owner_id: u64,
	pub release_id: u64,
	pub release_tag: String,
	pub asset_id: u64,
	pub asset_name: String,
	pub artifact_sha256: String,
	pub evidence: String,
	pub variant_id: String,
	pub adapter: String,
	pub format: String,
	pub root: String,
	pub entry_point: Option<String>,
	/// JSON array of the files the launcher owns.
	pub manifest: String,
	pub installed_at: i64,
	pub updated_at: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OperationRecord {
	pub id: OperationId,
	pub kind: String,
	pub installation_id: Option<InstallationId>,
	pub plan_digest: [u8; 32],
	pub plan: String,
	pub state: String,
	pub staging: Option<String>,
	pub detail: Option<String>,
	pub created_at: i64,
	pub updated_at: i64,
}

const INSTALLATION_COLUMNS: &str = "id, origin, catalog_slug, display_name, repository, repository_id, \
	owner_id, release_id, release_tag, asset_id, asset_name, artifact_sha256, evidence, variant_id, \
	adapter, format, root, entry_point, manifest, installed_at, updated_at";

const OPERATION_COLUMNS: &str =
	"id, kind, installation_id, plan_digest, plan, state, staging, detail, created_at, updated_at";

fn corrupt() -> rusqlite::Error {
	rusqlite::Error::InvalidQuery
}

fn id16(bytes: Vec<u8>) -> rusqlite::Result<[u8; 16]> {
	bytes.try_into().map_err(|_| corrupt())
}

fn unsigned(value: i64) -> rusqlite::Result<u64> {
	u64::try_from(value).map_err(|_| corrupt())
}

fn installation_from(row: &Row<'_>) -> rusqlite::Result<InstallationRecord> {
	Ok(InstallationRecord {
		id: InstallationId::from_bytes(id16(row.get(0)?)?),
		origin: row.get(1)?,
		catalog_slug: row.get(2)?,
		display_name: row.get(3)?,
		repository: row.get(4)?,
		repository_id: unsigned(row.get(5)?)?,
		owner_id: unsigned(row.get(6)?)?,
		release_id: unsigned(row.get(7)?)?,
		release_tag: row.get(8)?,
		asset_id: unsigned(row.get(9)?)?,
		asset_name: row.get(10)?,
		artifact_sha256: row.get(11)?,
		evidence: row.get(12)?,
		variant_id: row.get(13)?,
		adapter: row.get(14)?,
		format: row.get(15)?,
		root: row.get(16)?,
		entry_point: row.get(17)?,
		manifest: row.get(18)?,
		installed_at: row.get(19)?,
		updated_at: row.get(20)?,
	})
}

fn operation_from(row: &Row<'_>) -> rusqlite::Result<OperationRecord> {
	let installation: Option<Vec<u8>> = row.get(2)?;

	Ok(OperationRecord {
		id: OperationId::from_bytes(id16(row.get(0)?)?),
		kind: row.get(1)?,
		installation_id: installation
			.map(|bytes| id16(bytes).map(InstallationId::from_bytes))
			.transpose()?,
		plan_digest: row
			.get::<_, Vec<u8>>(3)?
			.try_into()
			.map_err(|_| corrupt())?,
		plan: row.get(4)?,
		state: row.get(5)?,
		staging: row.get(6)?,
		detail: row.get(7)?,
		created_at: row.get(8)?,
		updated_at: row.get(9)?,
	})
}

fn signed(value: u64) -> i64 {
	i64::try_from(value).unwrap_or(i64::MAX)
}

impl Database {
	pub async fn installations(&self) -> Result<Vec<InstallationRecord>, LauncherError> {
		self.reader()
			.call(|connection| -> DbResult<_> {
				connection
					.prepare_cached(&format!(
						"SELECT {INSTALLATION_COLUMNS} FROM installations ORDER BY display_name COLLATE NOCASE"
					))?
					.query_map([], installation_from)?
					.collect()
			})
			.await
			.map_err(unavailable)
	}

	pub async fn installation(
		&self,
		id: InstallationId,
	) -> Result<Option<InstallationRecord>, LauncherError> {
		self.reader()
			.call(move |connection| -> DbResult<_> {
				connection
					.prepare_cached(&format!(
						"SELECT {INSTALLATION_COLUMNS} FROM installations WHERE id = ?1"
					))?
					.query_row([id.as_bytes().as_slice()], installation_from)
					.optional()
			})
			.await
			.map_err(unavailable)
	}

	/// Operations that never reached a terminal state, oldest first.
	pub async fn unfinished_operations(&self) -> Result<Vec<OperationRecord>, LauncherError> {
		self.reader()
			.call(|connection| -> DbResult<_> {
				connection
					.prepare_cached(&format!(
						"SELECT {OPERATION_COLUMNS} FROM operations
						 WHERE state NOT IN ('completed', 'failed', 'cancelled')
						 ORDER BY created_at"
					))?
					.query_map([], operation_from)?
					.collect()
			})
			.await
			.map_err(unavailable)
	}

	/// The most recent operations, newest first, finished or not.
	pub async fn recent_operations(
		&self,
		limit: u32,
	) -> Result<Vec<OperationRecord>, LauncherError> {
		self.reader()
			.call(move |connection| -> DbResult<_> {
				connection
					.prepare_cached(&format!(
						"SELECT {OPERATION_COLUMNS} FROM operations ORDER BY created_at DESC, id DESC LIMIT ?1"
					))?
					.query_map([limit], operation_from)?
					.collect()
			})
			.await
			.map_err(unavailable)
	}

	pub async fn operation(
		&self,
		id: OperationId,
	) -> Result<Option<OperationRecord>, LauncherError> {
		self.writer()
			.call(move |connection| -> DbResult<_> {
				connection
					.prepare_cached(&format!(
						"SELECT {OPERATION_COLUMNS} FROM operations WHERE id = ?1"
					))?
					.query_row([id.as_bytes().as_slice()], operation_from)
					.optional()
			})
			.await
			.map_err(unavailable)
	}

	/// Writes a new journal entry before any destructive step starts.
	pub async fn begin_operation(&self, record: OperationRecord) -> Result<(), LauncherError> {
		self.writer()
			.call(move |connection| -> DbResult<()> {
				connection
					.prepare_cached(&format!(
						"INSERT INTO operations ({OPERATION_COLUMNS}) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)"
					))?
					.execute((
						record.id.as_bytes().as_slice(),
						record.kind,
						record.installation_id.map(|id| id.as_bytes().to_vec()),
						record.plan_digest.as_slice(),
						record.plan,
						record.state,
						record.staging,
						record.detail,
						record.created_at,
						record.updated_at,
					))?;

				Ok(())
			})
			.await
			.map_err(unavailable)
	}

	/// Moves an operation to `state`, optionally recording the staging path
	/// or a detail. Terminal states are final: they cannot be changed again.
	pub async fn set_operation_state(
		&self,
		id: OperationId,
		state: &'static str,
		staging: Option<String>,
		detail: Option<String>,
		now: i64,
	) -> Result<(), LauncherError> {
		let changed = self
			.writer()
			.call(move |connection| -> DbResult<usize> {
				connection
					.prepare_cached(
						"UPDATE operations
						 SET state = ?2, staging = coalesce(?3, staging), detail = coalesce(?4, detail), updated_at = ?5
						 WHERE id = ?1 AND state NOT IN ('completed', 'failed', 'cancelled')",
					)?
					.execute((id.as_bytes().as_slice(), state, staging, detail, now))
			})
			.await
			.map_err(unavailable)?;

		if changed == 0 {
			return Err(LauncherError::new(
				ErrorCode::InternalInvariant,
				"An operation that had already finished was changed.",
			));
		}

		Ok(())
	}

	/// Records a finished install or update and completes its journal entry
	/// in one transaction, so a crash leaves either both or neither.
	pub async fn commit_installation(
		&self,
		operation: OperationId,
		record: InstallationRecord,
		now: i64,
	) -> Result<(), LauncherError> {
		self.writer()
			.call(move |connection| -> DbResult<()> {
				let transaction = connection.transaction()?;

				transaction
					.prepare_cached(&format!(
						"INSERT INTO installations ({INSTALLATION_COLUMNS})
						 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21)
						 ON CONFLICT (id) DO UPDATE SET
						     release_id = excluded.release_id, release_tag = excluded.release_tag,
						     asset_id = excluded.asset_id, asset_name = excluded.asset_name,
						     artifact_sha256 = excluded.artifact_sha256, evidence = excluded.evidence,
						     variant_id = excluded.variant_id, adapter = excluded.adapter, format = excluded.format,
						     root = excluded.root, entry_point = excluded.entry_point,
						     manifest = excluded.manifest, updated_at = excluded.updated_at"
					))?
					.execute(rusqlite::params![
						record.id.as_bytes().as_slice(),
						record.origin,
						record.catalog_slug,
						record.display_name,
						record.repository,
						signed(record.repository_id),
						signed(record.owner_id),
						signed(record.release_id),
						record.release_tag,
						signed(record.asset_id),
						record.asset_name,
						record.artifact_sha256,
						record.evidence,
						record.variant_id,
						record.adapter,
						record.format,
						record.root,
						record.entry_point,
						record.manifest,
						record.installed_at,
						record.updated_at,
					])?;

				transaction
					.prepare_cached("UPDATE operations SET state = 'completed', updated_at = ?2 WHERE id = ?1")?
					.execute((operation.as_bytes().as_slice(), now))?;

				transaction.commit()
			})
			.await
			.map_err(unavailable)
	}

	/// Removes a receipt and completes the uninstall journal entry together.
	pub async fn commit_removal(
		&self,
		operation: OperationId,
		installation: InstallationId,
		detail: String,
		now: i64,
	) -> Result<(), LauncherError> {
		self.writer()
			.call(move |connection| -> DbResult<()> {
				let transaction = connection.transaction()?;

				transaction
					.prepare_cached("DELETE FROM installations WHERE id = ?1")?
					.execute([installation.as_bytes().as_slice()])?;
				transaction
					.prepare_cached(
						"UPDATE operations SET state = 'completed', detail = ?3, updated_at = ?2 WHERE id = ?1",
					)?
					.execute((operation.as_bytes().as_slice(), now, detail))?;

				transaction.commit()
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

	fn installation(id: InstallationId) -> InstallationRecord {
		InstallationRecord {
			id,
			origin: "custom".into(),
			catalog_slug: None,
			display_name: "App".into(),
			repository: "o/r".into(),
			repository_id: 1,
			owner_id: 2,
			release_id: 3,
			release_tag: "v1".into(),
			asset_id: 4,
			asset_name: "app.zip".into(),
			artifact_sha256: "ab".repeat(32),
			evidence: "local-observed".into(),
			variant_id: "custom".into(),
			adapter: "portable-v1".into(),
			format: "zip".into(),
			root: "/apps/x".into(),
			entry_point: Some("bin/app".into()),
			manifest: "[]".into(),
			installed_at: 10,
			updated_at: 10,
		}
	}

	fn operation(id: OperationId, installation: Option<InstallationId>) -> OperationRecord {
		OperationRecord {
			id,
			kind: "install".into(),
			installation_id: installation,
			plan_digest: [7; 32],
			plan: "{}".into(),
			state: "accepted".into(),
			staging: None,
			detail: None,
			created_at: 1,
			updated_at: 1,
		}
	}

	#[tokio::test]
	async fn a_journal_entry_survives_and_is_listed_as_unfinished() {
		let (_directory, database) = open_temporary().await;
		let id = OperationId::new();

		database.begin_operation(operation(id, None)).await.unwrap();
		database
			.set_operation_state(id, "downloading", Some("/staging/x".into()), None, 2)
			.await
			.unwrap();

		let unfinished = database.unfinished_operations().await.unwrap();

		assert_eq!(unfinished.len(), 1);
		assert_eq!(unfinished[0].state, "downloading");
		assert_eq!(unfinished[0].staging.as_deref(), Some("/staging/x"));
	}

	#[tokio::test]
	async fn recent_operations_come_newest_first_and_respect_the_limit() {
		let (_directory, database) = open_temporary().await;

		for created_at in [10, 30, 20] {
			let mut record = operation(OperationId::new(), None);
			record.created_at = created_at;
			database.begin_operation(record).await.unwrap();
		}

		let recent = database.recent_operations(2).await.unwrap();

		assert_eq!(
			recent.iter().map(|r| r.created_at).collect::<Vec<_>>(),
			vec![30, 20]
		);
	}

	#[tokio::test]
	async fn terminal_states_cannot_be_reopened() {
		let (_directory, database) = open_temporary().await;
		let id = OperationId::new();
		database.begin_operation(operation(id, None)).await.unwrap();
		database
			.set_operation_state(id, "failed", None, Some("boom".into()), 2)
			.await
			.unwrap();

		assert!(
			database
				.set_operation_state(id, "downloading", None, None, 3)
				.await
				.is_err()
		);
		assert!(database.unfinished_operations().await.unwrap().is_empty());
	}

	#[tokio::test]
	async fn committing_records_the_receipt_and_finishes_the_journal_together() {
		let (_directory, database) = open_temporary().await;
		let (op, app) = (OperationId::new(), InstallationId::new());
		database
			.begin_operation(operation(op, Some(app)))
			.await
			.unwrap();

		database
			.commit_installation(op, installation(app), 5)
			.await
			.unwrap();

		assert_eq!(
			database.installation(app).await.unwrap(),
			Some(installation(app))
		);
		assert!(database.unfinished_operations().await.unwrap().is_empty());
		assert_eq!(
			database.operation(op).await.unwrap().unwrap().state,
			"completed"
		);
	}

	#[tokio::test]
	async fn an_update_replaces_the_version_but_keeps_the_install_date() {
		let (_directory, database) = open_temporary().await;
		let app = InstallationId::new();

		database
			.begin_operation(operation(OperationId::new(), Some(app)))
			.await
			.unwrap();
		let first = OperationId::new();
		database
			.begin_operation(operation(first, Some(app)))
			.await
			.unwrap();
		database
			.commit_installation(first, installation(app), 10)
			.await
			.unwrap();

		let second = OperationId::new();
		database
			.begin_operation(operation(second, Some(app)))
			.await
			.unwrap();
		let mut newer = installation(app);
		newer.release_tag = "v2".into();
		newer.installed_at = 99;
		newer.updated_at = 99;
		database
			.commit_installation(second, newer, 99)
			.await
			.unwrap();

		let stored = database.installation(app).await.unwrap().unwrap();
		assert_eq!(stored.release_tag, "v2");
		assert_eq!(stored.installed_at, 10);
		assert_eq!(stored.updated_at, 99);
	}

	#[tokio::test]
	async fn removal_deletes_the_receipt_and_finishes_the_journal() {
		let (_directory, database) = open_temporary().await;
		let (install_op, app) = (OperationId::new(), InstallationId::new());
		database
			.begin_operation(operation(install_op, Some(app)))
			.await
			.unwrap();
		database
			.commit_installation(install_op, installation(app), 5)
			.await
			.unwrap();

		let remove_op = OperationId::new();
		database
			.begin_operation(operation(remove_op, Some(app)))
			.await
			.unwrap();
		database
			.commit_removal(remove_op, app, "removed 3 files".into(), 6)
			.await
			.unwrap();

		assert_eq!(database.installation(app).await.unwrap(), None);
		assert!(database.installations().await.unwrap().is_empty());
		assert_eq!(
			database
				.operation(remove_op)
				.await
				.unwrap()
				.unwrap()
				.detail
				.as_deref(),
			Some("removed 3 files")
		);
	}
}
