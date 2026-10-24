//! Update policy, asset observations, and decision history.

use launcher_model::{InstallationId, LauncherError, UpdateMode};
use rusqlite::OptionalExtension;

use crate::Database;
use crate::error::{DbResult, unavailable};

/// One app's automatic-update preferences. Absent fields mean "use the
/// default".
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AppPolicy {
	/// `None` follows the global minimum age.
	pub min_age_seconds: Option<u64>,
	pub auto_paused: bool,
	/// A pinned app never updates automatically.
	pub pinned_release_id: Option<u64>,
	pub allow_prerelease: bool,
	/// An explicit opt-in for a repository Remade has not reviewed.
	pub unattended_custom: bool,
	/// This app's own level on the update chain. `None` follows the global
	/// mode. A pin is `pinned_release_id`, never this.
	pub update_mode: Option<UpdateMode>,
}

/// What the provider reported about one asset.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ObservedAsset {
	pub repository_id: u64,
	pub release_id: u64,
	pub asset_id: u64,
	pub tag: String,
	pub size: u64,
	pub host_sha256: Option<String>,
	pub release_published_at: Option<i64>,
	pub asset_created_at: Option<i64>,
	pub asset_updated_at: Option<i64>,
}

impl ObservedAsset {
	/// Identity of the bytes as far as the provider tells us. When it changes,
	/// the asset is a different thing and its age starts over.
	fn fingerprint(&self) -> String {
		format!(
			"{}|{}|{}|{}",
			self.size,
			self.host_sha256.as_deref().unwrap_or("-"),
			self.asset_created_at.unwrap_or(0),
			self.asset_updated_at.unwrap_or(0),
		)
	}

	/// The latest provider timestamp that touches this asset.
	fn provider_time(&self) -> Option<i64> {
		[
			self.release_published_at,
			self.asset_created_at,
			self.asset_updated_at,
		]
		.into_iter()
		.flatten()
		.max()
	}
}

/// An observation as stored.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AssetObservation {
	pub first_seen_at: i64,
	pub last_seen_at: i64,
	/// When this asset's minimum-age clock started.
	pub window_start: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DecisionRecord {
	pub installation_id: InstallationId,
	pub cycle_id: String,
	pub trigger: String,
	pub candidate_release_id: Option<u64>,
	pub candidate_asset_id: Option<u64>,
	pub outcome: String,
	pub reason: String,
	pub policy: String,
	pub plan_digest: Option<[u8; 32]>,
	pub decided_at: i64,
}

fn signed(value: u64) -> i64 {
	i64::try_from(value).unwrap_or(i64::MAX)
}

impl Database {
	pub async fn app_policy(
		&self,
		installation: InstallationId,
	) -> Result<AppPolicy, LauncherError> {
		self.reader()
			.call(move |connection| -> DbResult<_> {
				connection
					.prepare_cached(
						"SELECT min_age_seconds, auto_paused, pinned_release_id, allow_prerelease, unattended_custom, update_mode
						 FROM app_update_policy WHERE installation_id = ?1",
					)?
					.query_row([installation.as_bytes().as_slice()], |row| {
						Ok(AppPolicy {
							min_age_seconds: row.get::<_, Option<i64>>(0)?.and_then(|v| u64::try_from(v).ok()),
							auto_paused: row.get::<_, i64>(1)? != 0,
							pinned_release_id: row.get::<_, Option<i64>>(2)?.and_then(|v| u64::try_from(v).ok()),
							allow_prerelease: row.get::<_, i64>(3)? != 0,
							unattended_custom: row.get::<_, i64>(4)? != 0,
							update_mode: row
								.get::<_, Option<String>>(5)?
								.and_then(|text| text.parse().ok()),
						})
					})
					.optional()
					.map(Option::unwrap_or_default)
			})
			.await
			.map_err(unavailable)
	}

	pub async fn set_app_policy(
		&self,
		installation: InstallationId,
		policy: AppPolicy,
	) -> Result<(), LauncherError> {
		self.writer()
			.call(move |connection| -> DbResult<()> {
				connection
					.prepare_cached(
						"INSERT INTO app_update_policy
						     (installation_id, min_age_seconds, auto_paused, pinned_release_id, allow_prerelease, unattended_custom, update_mode)
						 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
						 ON CONFLICT (installation_id) DO UPDATE SET
						     min_age_seconds = excluded.min_age_seconds, auto_paused = excluded.auto_paused,
						     pinned_release_id = excluded.pinned_release_id, allow_prerelease = excluded.allow_prerelease,
						     unattended_custom = excluded.unattended_custom, update_mode = excluded.update_mode",
					)?
					.execute((
						installation.as_bytes().as_slice(),
						policy.min_age_seconds.map(signed),
						i64::from(policy.auto_paused),
						policy.pinned_release_id.map(signed),
						i64::from(policy.allow_prerelease),
						i64::from(policy.unattended_custom),
						policy.update_mode.map(UpdateMode::as_str),
					))?;

				Ok(())
			})
			.await
			.map_err(unavailable)
	}

	/// Forgets every app's own update preferences, so each follows the global
	/// settings again. Pins go with them.
	pub async fn clear_app_policies(&self) -> Result<(), LauncherError> {
		self.writer()
			.call(|connection| -> DbResult<()> {
				connection.execute("DELETE FROM app_update_policy", [])?;

				Ok(())
			})
			.await
			.map_err(unavailable)
	}

	/// Records that `asset` was seen at `now` and returns where its
	/// minimum-age clock starts.
	///
	/// A first sighting starts the window at the provider's own timestamps
	/// when it has them, and at `now` when it does not. A later sighting with
	/// a different fingerprint, such as a replaced file, restarts the window
	/// at the newer of the provider's time and `now`, so a swapped asset on
	/// an old release never inherits that release's age.
	pub async fn observe_asset(
		&self,
		asset: ObservedAsset,
		now: i64,
	) -> Result<AssetObservation, LauncherError> {
		self.writer()
			.call(move |connection| -> DbResult<AssetObservation> {
				let transaction = connection.transaction()?;
				let fingerprint = asset.fingerprint();

				let existing: Option<(String, i64, i64)> = transaction
					.prepare_cached(
						"SELECT fingerprint, first_seen_at, window_start FROM asset_observations
						 WHERE repository_id = ?1 AND release_id = ?2 AND asset_id = ?3",
					)?
					.query_row((signed(asset.repository_id), signed(asset.release_id), signed(asset.asset_id)), |row| {
						Ok((row.get(0)?, row.get(1)?, row.get(2)?))
					})
					.optional()?;

				let (first_seen, window_start) = match existing {
					Some((stored, first_seen, window)) if stored == fingerprint => (first_seen, window),
					Some((_, first_seen, _)) => (first_seen, asset.provider_time().map_or(now, |t| t.max(now))),
					// Never seen: the provider's time is the honest start, and with
					// no provider time the first sighting is.
					None => (now, asset.provider_time().unwrap_or(now)),
				};

				transaction
					.prepare_cached(
						"INSERT INTO asset_observations
						     (repository_id, release_id, asset_id, tag, fingerprint, size, host_sha256,
						      release_published_at, asset_created_at, asset_updated_at,
						      first_seen_at, last_seen_at, window_start)
						 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
						 ON CONFLICT (repository_id, release_id, asset_id) DO UPDATE SET
						     tag = excluded.tag, fingerprint = excluded.fingerprint, size = excluded.size,
						     host_sha256 = excluded.host_sha256, release_published_at = excluded.release_published_at,
						     asset_created_at = excluded.asset_created_at, asset_updated_at = excluded.asset_updated_at,
						     last_seen_at = excluded.last_seen_at, window_start = excluded.window_start",
					)?
					.execute(rusqlite::params![
						signed(asset.repository_id),
						signed(asset.release_id),
						signed(asset.asset_id),
						asset.tag,
						fingerprint,
						signed(asset.size),
						asset.host_sha256,
						asset.release_published_at,
						asset.asset_created_at,
						asset.asset_updated_at,
						first_seen,
						now,
						window_start,
					])?;

				transaction.commit()?;

				Ok(AssetObservation {
					first_seen_at: first_seen,
					last_seen_at: now,
					window_start,
				})
			})
			.await
			.map_err(unavailable)
	}

	pub async fn record_decision(&self, decision: DecisionRecord) -> Result<(), LauncherError> {
		self.writer()
			.call(move |connection| -> DbResult<()> {
				connection
					.prepare_cached(
						"INSERT INTO update_decisions
						     (installation_id, cycle_id, trigger_kind, candidate_release_id, candidate_asset_id,
						      outcome, reason, policy, plan_digest, decided_at)
						 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
					)?
					.execute((
						decision.installation_id.as_bytes().as_slice(),
						decision.cycle_id,
						decision.trigger,
						decision.candidate_release_id.map(signed),
						decision.candidate_asset_id.map(signed),
						decision.outcome,
						decision.reason,
						decision.policy,
						decision.plan_digest.map(|digest| digest.to_vec()),
						decision.decided_at,
					))?;

				Ok(())
			})
			.await
			.map_err(unavailable)
	}

	/// The latest decisions for one app, newest first.
	pub async fn recent_decisions(
		&self,
		installation: InstallationId,
		limit: u32,
	) -> Result<Vec<DecisionRecord>, LauncherError> {
		self.reader()
			.call(move |connection| -> DbResult<_> {
				connection
					.prepare_cached(
						"SELECT cycle_id, trigger_kind, candidate_release_id, candidate_asset_id, outcome, reason,
						        policy, plan_digest, decided_at
						 FROM update_decisions WHERE installation_id = ?1
						 ORDER BY decided_at DESC, id DESC LIMIT ?2",
					)?
					.query_map((installation.as_bytes().as_slice(), limit), |row| {
						let digest: Option<Vec<u8>> = row.get(7)?;

						Ok(DecisionRecord {
							installation_id: installation,
							cycle_id: row.get(0)?,
							trigger: row.get(1)?,
							candidate_release_id: row.get::<_, Option<i64>>(2)?.and_then(|v| u64::try_from(v).ok()),
							candidate_asset_id: row.get::<_, Option<i64>>(3)?.and_then(|v| u64::try_from(v).ok()),
							outcome: row.get(4)?,
							reason: row.get(5)?,
							policy: row.get(6)?,
							plan_digest: digest.and_then(|bytes| bytes.try_into().ok()),
							decided_at: row.get(8)?,
						})
					})?
					.collect()
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

	fn asset() -> ObservedAsset {
		ObservedAsset {
			repository_id: 1,
			release_id: 2,
			asset_id: 3,
			tag: "v1".into(),
			size: 100,
			host_sha256: Some("ab".repeat(32)),
			release_published_at: Some(1_000),
			asset_created_at: Some(1_000),
			asset_updated_at: Some(1_000),
		}
	}

	#[tokio::test]
	async fn an_app_without_a_saved_policy_gets_the_defaults() {
		let (_directory, database) = open_temporary().await;

		assert_eq!(
			database.app_policy(InstallationId::new()).await.unwrap(),
			AppPolicy::default()
		);
	}

	#[tokio::test]
	async fn a_policy_round_trips_and_can_be_changed() {
		let (_directory, database) = open_temporary().await;
		let app = InstallationId::new();
		let policy = AppPolicy {
			min_age_seconds: Some(86_400),
			auto_paused: true,
			pinned_release_id: Some(9),
			allow_prerelease: true,
			unattended_custom: false,
			update_mode: Some(UpdateMode::OnLaunch),
		};

		database.set_app_policy(app, policy.clone()).await.unwrap();
		assert_eq!(database.app_policy(app).await.unwrap(), policy);

		database
			.set_app_policy(app, AppPolicy::default())
			.await
			.unwrap();
		assert_eq!(
			database.app_policy(app).await.unwrap(),
			AppPolicy::default()
		);
	}

	#[tokio::test]
	async fn the_window_starts_at_the_providers_time_not_at_first_sight() {
		let (_directory, database) = open_temporary().await;

		// Published at 1000, first seen much later: not waiting an extra hour
		// just because this device is late to the news.
		let observation = database.observe_asset(asset(), 50_000).await.unwrap();

		assert_eq!(observation.window_start, 1_000);
		assert_eq!(observation.first_seen_at, 50_000);
	}

	#[tokio::test]
	async fn the_latest_provider_timestamp_wins() {
		let (_directory, database) = open_temporary().await;
		let mut replaced = asset();
		replaced.asset_updated_at = Some(9_000);

		assert_eq!(
			database
				.observe_asset(replaced, 50_000)
				.await
				.unwrap()
				.window_start,
			9_000
		);
	}

	#[tokio::test]
	async fn without_provider_times_the_first_sighting_starts_the_clock() {
		let (_directory, database) = open_temporary().await;
		let mut bare = asset();
		bare.release_published_at = None;
		bare.asset_created_at = None;
		bare.asset_updated_at = None;

		assert_eq!(
			database
				.observe_asset(bare, 777)
				.await
				.unwrap()
				.window_start,
			777
		);
	}

	#[tokio::test]
	async fn seeing_the_same_asset_again_keeps_its_window() {
		let (_directory, database) = open_temporary().await;

		database.observe_asset(asset(), 5_000).await.unwrap();
		let again = database.observe_asset(asset(), 90_000).await.unwrap();

		assert_eq!(again.window_start, 1_000);
		assert_eq!(again.first_seen_at, 5_000);
		assert_eq!(again.last_seen_at, 90_000);
	}

	#[tokio::test]
	async fn a_replaced_asset_restarts_the_clock_even_on_an_old_release() {
		let (_directory, database) = open_temporary().await;
		database.observe_asset(asset(), 5_000).await.unwrap();

		let mut swapped = asset();
		swapped.host_sha256 = Some("cd".repeat(32));

		let observation = database.observe_asset(swapped, 70_000).await.unwrap();

		assert_eq!(
			observation.window_start, 70_000,
			"the old release's age must not carry over"
		);
	}

	#[tokio::test]
	async fn decisions_are_listed_newest_first() {
		let (_directory, database) = open_temporary().await;
		let app = InstallationId::new();

		for (at, outcome) in [(10, "skipped"), (30, "applied"), (20, "skipped")] {
			database
				.record_decision(DecisionRecord {
					installation_id: app,
					cycle_id: "c".into(),
					trigger: "on-launch".into(),
					candidate_release_id: Some(2),
					candidate_asset_id: Some(3),
					outcome: outcome.into(),
					reason: "test".into(),
					policy: "{}".into(),
					plan_digest: Some([1; 32]),
					decided_at: at,
				})
				.await
				.unwrap();
		}

		let decisions = database.recent_decisions(app, 2).await.unwrap();

		assert_eq!(
			decisions.iter().map(|d| d.decided_at).collect::<Vec<_>>(),
			vec![30, 20]
		);
		assert_eq!(decisions[0].outcome, "applied");
	}
}
