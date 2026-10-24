//! Saved plans and recorded consents.

use launcher_model::{ErrorCode, InstallationId, LauncherError};

use crate::Database;
use crate::error::{DbResult, unavailable};

/// A sealed plan and the text that goes with it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StoredPlan {
	/// Short form of the digest, what a person types.
	pub id: String,
	pub digest: [u8; 32],
	/// The sealed body, as JSON.
	pub body: String,
	/// Presentation text and metadata, as JSON. Not covered by the digest.
	pub extra: String,
	pub update_of: Option<InstallationId>,
	pub created_at: i64,
	pub expires_at: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConsentRecord {
	pub kind: String,
	pub subject: String,
	pub scope: String,
	pub policy_version: u32,
	pub actor: String,
	pub granted_at: i64,
}

impl Database {
	/// Saves a plan. Saving the same plan twice is harmless.
	pub async fn save_plan(&self, plan: StoredPlan) -> Result<(), LauncherError> {
		self.writer()
			.call(move |connection| -> DbResult<()> {
				connection
					.prepare_cached(
						"INSERT INTO plans (id, digest, body, extra, update_of, created_at, expires_at)
						 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
						 ON CONFLICT (digest) DO NOTHING",
					)?
					.execute((
						plan.id,
						plan.digest.as_slice(),
						plan.body,
						plan.extra,
						plan.update_of.map(|id| id.as_bytes().to_vec()),
						plan.created_at,
						plan.expires_at,
					))?;

				Ok(())
			})
			.await
			.map_err(unavailable)
	}

	/// Finds a plan by its ID, or by any unambiguous start of it.
	pub async fn find_plan(&self, id: &str) -> Result<Option<StoredPlan>, LauncherError> {
		// A prefix match without LIKE, so `%` and `_` mean themselves. An empty
		// prefix would match every plan, so it matches none.
		if id.is_empty() {
			return Ok(None);
		}

		let prefix = id.to_owned();
		let rows: Vec<StoredPlan> = self
			.reader()
			.call(move |connection| -> DbResult<_> {
				connection
					.prepare_cached(
						"SELECT id, digest, body, extra, update_of, created_at, expires_at
						 FROM plans WHERE substr(id, 1, length(?1)) = ?1 LIMIT 2",
					)?
					.query_map([prefix], |row| {
						let digest: Vec<u8> = row.get(1)?;
						let update_of: Option<Vec<u8>> = row.get(4)?;

						Ok(StoredPlan {
							id: row.get(0)?,
							digest: digest
								.try_into()
								.map_err(|_| rusqlite::Error::InvalidQuery)?,
							body: row.get(2)?,
							extra: row.get(3)?,
							update_of: update_of
								.map(|bytes| {
									bytes
										.try_into()
										.map(InstallationId::from_bytes)
										.map_err(|_| rusqlite::Error::InvalidQuery)
								})
								.transpose()?,
							created_at: row.get(5)?,
							expires_at: row.get(6)?,
						})
					})?
					.collect()
			})
			.await
			.map_err(unavailable)?;

		match rows.len() {
			0 => Ok(None),
			1 => Ok(rows.into_iter().next()),
			_ => Err(LauncherError::new(
				ErrorCode::InvalidSourceUrl,
				"That plan ID matches more than one plan. Type more of it.",
			)),
		}
	}

	/// Deletes plans that expired before `now`.
	pub async fn purge_expired_plans(&self, now: i64) -> Result<usize, LauncherError> {
		self.writer()
			.call(move |connection| -> DbResult<usize> {
				connection
					.prepare_cached("DELETE FROM plans WHERE expires_at < ?1")?
					.execute([now])
			})
			.await
			.map_err(unavailable)
	}

	/// Records a decision a person made.
	pub async fn record_consent(&self, consent: ConsentRecord) -> Result<(), LauncherError> {
		self.writer()
			.call(move |connection| -> DbResult<()> {
				connection
					.prepare_cached(
						"INSERT INTO consents (kind, subject, scope, policy_version, actor, granted_at)
						 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
					)?
					.execute((
						consent.kind,
						consent.subject,
						consent.scope,
						consent.policy_version,
						consent.actor,
						consent.granted_at,
					))?;

				Ok(())
			})
			.await
			.map_err(unavailable)
	}

	/// Active consents of `kind` for `subject`, newest first.
	pub async fn consents(
		&self,
		kind: &str,
		subject: &str,
	) -> Result<Vec<ConsentRecord>, LauncherError> {
		let (kind, subject) = (kind.to_owned(), subject.to_owned());

		self.reader()
			.call(move |connection| -> DbResult<_> {
				connection
					.prepare_cached(
						"SELECT kind, subject, scope, policy_version, actor, granted_at
						 FROM consents WHERE kind = ?1 AND subject = ?2 AND revoked_at IS NULL
						 ORDER BY granted_at DESC, id DESC",
					)?
					.query_map((kind, subject), |row| {
						Ok(ConsentRecord {
							kind: row.get(0)?,
							subject: row.get(1)?,
							scope: row.get(2)?,
							policy_version: row.get(3)?,
							actor: row.get(4)?,
							granted_at: row.get(5)?,
						})
					})?
					.collect()
			})
			.await
			.map_err(unavailable)
	}

	/// Withdraws every active consent of `kind` for `subject`.
	pub async fn revoke_consents(
		&self,
		kind: &str,
		subject: &str,
		now: i64,
	) -> Result<(), LauncherError> {
		let (kind, subject) = (kind.to_owned(), subject.to_owned());

		self.writer()
			.call(move |connection| -> DbResult<()> {
				connection
					.prepare_cached(
						"UPDATE consents SET revoked_at = ?3
						 WHERE kind = ?1 AND subject = ?2 AND revoked_at IS NULL",
					)?
					.execute((kind, subject, now))?;

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

	fn plan(id: &str, digest_byte: u8, expires_at: i64) -> StoredPlan {
		StoredPlan {
			id: id.into(),
			digest: [digest_byte; 32],
			body: "{}".into(),
			extra: "{}".into(),
			update_of: None,
			created_at: 1,
			expires_at,
		}
	}

	#[tokio::test]
	async fn a_saved_plan_is_found_by_id_or_prefix() {
		let (_directory, database) = open_temporary().await;
		database
			.save_plan(plan("abcdef0123456789", 1, 100))
			.await
			.unwrap();

		assert!(
			database
				.find_plan("abcdef0123456789")
				.await
				.unwrap()
				.is_some()
		);
		assert!(database.find_plan("abcd").await.unwrap().is_some());
		assert!(database.find_plan("zzzz").await.unwrap().is_none());
	}

	#[tokio::test]
	async fn an_ambiguous_prefix_is_refused_not_guessed() {
		let (_directory, database) = open_temporary().await;
		database.save_plan(plan("abc111", 1, 100)).await.unwrap();
		database.save_plan(plan("abc222", 2, 100)).await.unwrap();

		assert!(database.find_plan("abc").await.is_err());
		assert!(database.find_plan("abc1").await.unwrap().is_some());
	}

	#[tokio::test]
	async fn wildcards_and_empty_lookups_match_nothing() {
		let (_directory, database) = open_temporary().await;
		database.save_plan(plan("abc111", 1, 100)).await.unwrap();

		for lookup in ["%", "_", "a_c", "%%", ""] {
			assert!(
				database.find_plan(lookup).await.unwrap().is_none(),
				"{lookup:?}"
			);
		}
	}

	#[tokio::test]
	async fn saving_the_same_plan_twice_changes_nothing() {
		let (_directory, database) = open_temporary().await;

		database.save_plan(plan("abc111", 1, 100)).await.unwrap();
		database.save_plan(plan("abc111", 1, 100)).await.unwrap();

		assert!(database.find_plan("abc111").await.unwrap().is_some());
	}

	#[tokio::test]
	async fn expired_plans_are_purged() {
		let (_directory, database) = open_temporary().await;
		database.save_plan(plan("old111", 1, 50)).await.unwrap();
		database.save_plan(plan("new222", 2, 500)).await.unwrap();

		assert_eq!(database.purge_expired_plans(100).await.unwrap(), 1);
		assert!(database.find_plan("old111").await.unwrap().is_none());
		assert!(database.find_plan("new222").await.unwrap().is_some());
	}

	#[tokio::test]
	async fn consents_are_recorded_listed_and_revocable() {
		let (_directory, database) = open_temporary().await;
		let consent = ConsentRecord {
			kind: "update-mode".into(),
			subject: "daemon".into(),
			scope: "this user".into(),
			policy_version: 1,
			actor: "cli".into(),
			granted_at: 10,
		};

		database.record_consent(consent.clone()).await.unwrap();

		assert_eq!(
			database.consents("update-mode", "daemon").await.unwrap(),
			vec![consent]
		);
		assert!(
			database
				.consents("update-mode", "on-launch")
				.await
				.unwrap()
				.is_empty()
		);

		database
			.revoke_consents("update-mode", "daemon", 20)
			.await
			.unwrap();

		assert!(
			database
				.consents("update-mode", "daemon")
				.await
				.unwrap()
				.is_empty()
		);
	}
}
