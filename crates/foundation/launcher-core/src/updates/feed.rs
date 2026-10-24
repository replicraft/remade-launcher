//! Reading the website's daily release snapshot.
//!
//! One small signed file stands in for a couple of GitHub requests per
//! installed app. It is fetched at most every few hours, kept on disk, and
//! checked again every time it is loaded: the data folder is not a trust
//! anchor. When it is missing, stale, or refused, update checks simply ask
//! GitHub as they always did.

use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use std::time::Duration;

use launcher_catalog::{
	HighWater, MAX_OBSERVATIONS_BYTES, ObservationsContext, VerifiedObservations, limits,
	verify_observations,
};
use launcher_model::{ErrorCode, LauncherError};
use time::OffsetDateTime;

use crate::engine::{Engine, now_unix};

/// How long a fetched snapshot is used before asking again. The website makes
/// a new one daily, so asking more often only repeats the same answer.
const REFRESH_SECONDS: i64 = 6 * 60 * 60;

/// Never let a slow server hold up an update check.
const FETCH_BUDGET: Duration = Duration::from_secs(15);

const HIGH_WATER_KEY: &str = "catalog.updates.high_water";
const FETCHED_KEY: &str = "catalog.updates.fetched_at";

struct Held {
	snapshot: Arc<VerifiedObservations>,
	fetched_at: i64,
}

/// The snapshot in memory, and a gate so concurrent lookups fetch it once.
#[derive(Default)]
pub(crate) struct ObservationsCache {
	held: RwLock<Option<Held>>,
	gate: tokio::sync::Mutex<()>,
}

impl ObservationsCache {
	fn current(&self, now: i64) -> Option<Arc<VerifiedObservations>> {
		let held = self.held.read().ok()?;
		let held = held.as_ref()?;
		let unexpired = held.snapshot.expires_at().unix_timestamp() > now;

		(unexpired && now - held.fetched_at < REFRESH_SECONDS).then(|| Arc::clone(&held.snapshot))
	}

	fn hold(&self, snapshot: Arc<VerifiedObservations>, fetched_at: i64) {
		if let Ok(mut held) = self.held.write() {
			*held = Some(Held {
				snapshot,
				fetched_at,
			});
		}
	}

	/// Puts a snapshot in directly, for tests.
	#[cfg(test)]
	pub(crate) fn set_for_test(&self, snapshot: VerifiedObservations) {
		self.hold(Arc::new(snapshot), now_unix());
	}
}

impl Engine {
	fn observations_files(&self) -> (PathBuf, PathBuf) {
		(
			self.data.root().join("catalog-updates.json"),
			self.data.root().join("catalog-updates.sig"),
		)
	}

	/// The snapshot, if a current and verified one can be had. Never fails:
	/// `None` means "ask GitHub".
	pub(crate) async fn observations(&self) -> Option<Arc<VerifiedObservations>> {
		let now = now_unix();

		if let Some(snapshot) = self.observations.current(now) {
			return Some(snapshot);
		}

		self.catalog().config().updates_feed.as_ref()?;

		let _gate = self.observations.gate.lock().await;

		// Another lookup may have fetched it while this one waited.
		if let Some(snapshot) = self.observations.current(now_unix()) {
			return Some(snapshot);
		}

		match tokio::time::timeout(FETCH_BUDGET, self.load_observations()).await {
			Ok(Ok(snapshot)) => Some(snapshot),
			Ok(Err(error)) => {
				tracing::info!(%error, "the release snapshot could not be used");

				None
			}
			Err(_) => {
				tracing::info!("the release snapshot took too long");

				None
			}
		}
	}

	async fn load_observations(&self) -> Result<Arc<VerifiedObservations>, LauncherError> {
		let (json, signature) = self.observations_files();

		// The copy on disk, if it is recent enough to use as it is.
		let fetched_at = self
			.state_value(FETCHED_KEY)
			.await
			.and_then(|text| text.parse::<i64>().ok());

		if let Some(at) = fetched_at.filter(|at| now_unix() - at < REFRESH_SECONDS)
			&& let (Ok(manifest), Ok(envelope)) = (
				tokio::fs::read(&json).await,
				tokio::fs::read(&signature).await,
			) && let Ok(snapshot) = self.accept_observations(&manifest, &envelope).await
		{
			let snapshot = Arc::new(snapshot);

			self.observations.hold(Arc::clone(&snapshot), at);

			return Ok(snapshot);
		}

		let urls = self
			.catalog()
			.config()
			.updates_feed
			.clone()
			.ok_or_else(|| LauncherError::new(ErrorCode::ProviderNotFound, "No snapshot."))?;
		let (manifest, envelope) = tokio::join!(
			self.http
				.get_bounded(urls.manifest_url, MAX_OBSERVATIONS_BYTES),
			self.http
				.get_bounded(urls.envelope_url, limits::MAX_ENVELOPE_BYTES),
		);
		let (manifest, envelope) = (manifest?, envelope?);
		let snapshot = Arc::new(self.accept_observations(&manifest, &envelope).await?);
		let now = now_unix();

		// Kept for the next start. A failure to save is not a failure to use.
		let _ = tokio::fs::write(&json, &manifest).await;
		let _ = tokio::fs::write(&signature, &envelope).await;
		let _ = self.set_state_value(FETCHED_KEY, now.to_string()).await;

		self.observations.hold(Arc::clone(&snapshot), now);

		Ok(snapshot)
	}

	/// Checks a snapshot, and remembers it so an older one is never accepted.
	pub(crate) async fn accept_observations(
		&self,
		manifest: &[u8],
		envelope: &[u8],
	) -> Result<VerifiedObservations, LauncherError> {
		let high_water = self.state_value(HIGH_WATER_KEY).await.and_then(|text| {
			let (revision, digest) = text.split_once(':')?;
			let digest: [u8; 32] = hex::decode(digest).ok()?.try_into().ok()?;

			Some(HighWater {
				revision: revision.parse().ok()?,
				digest: Some(digest),
			})
		});
		let config = self.catalog().config();
		let snapshot = verify_observations(
			manifest,
			envelope,
			&ObservationsContext {
				registry: &config.registry,
				feed_id: &config.feed_id,
				now: OffsetDateTime::now_utc(),
				high_water,
			},
		)
		.map_err(|error| {
			LauncherError::new(
				ErrorCode::ReleaseRejected,
				"The release snapshot did not pass its checks.",
			)
			.with_source(error)
		})?;

		let _ = self
			.set_state_value(
				HIGH_WATER_KEY,
				format!("{}:{}", snapshot.revision(), hex::encode(snapshot.digest())),
			)
			.await;

		Ok(snapshot)
	}
}
