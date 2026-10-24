//! The catalog service: fetch, verify, persist, and publish the result.
//!
//! Everything CPU-heavy (parsing, hashing, signature checks, per-project
//! validation) runs on the blocking pool, and rayon fans the per-project
//! work out across cores. Nothing here runs on the interface thread.

use std::sync::Arc;

use launcher_catalog::{
	CatalogError, Change, ErrorKind, Freshness, FreshnessPolicy, HighWater, VerifiedCatalog,
	VerifyContext, limits, verify,
};
use launcher_net::HttpClient;
use launcher_storage::{Database, StoredCatalog};
use time::OffsetDateTime;
use tokio::sync::watch;

use crate::config::CatalogConfig;

/// How far the clock may fall behind the last trusted time before the
/// interface warns about it.
const CLOCK_ROLLBACK_TOLERANCE_SECONDS: i64 = 24 * 60 * 60;

/// How many times a mismatched manifest/signature pair is refetched. A CDN
/// can briefly serve a new manifest beside an old signature.
const PAIR_ATTEMPTS: usize = 2;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Origin {
	/// Fetched and verified during this run.
	Network { at: OffsetDateTime },
	/// Loaded from the local copy, which was re-verified on the way in.
	Cache { accepted_at: OffsetDateTime },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProblemKind {
	/// Could not reach the server. Nothing about trust is implied.
	Network,
	/// Bytes arrived but failed an authenticity or rollback check.
	Security,
	/// Authentic but unusable: expired, wrong schema, unsupported feature.
	Unusable,
	/// This build has no catalog keys to verify with.
	NoTrustedKeys,
	/// The local database failed.
	Storage,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Problem {
	pub kind: ProblemKind,
	/// A stable, machine-readable code for scripts.
	pub code: &'static str,
	/// Safe to display. Never contains server-provided text.
	pub message: String,
}

#[derive(Clone, Debug, Default)]
pub struct CatalogState {
	pub catalog: Option<Arc<VerifiedCatalog>>,
	pub origin: Option<Origin>,
	/// The outcome of the latest refresh, if it failed.
	pub problem: Option<Problem>,
	pub refreshing: bool,
	/// The system clock is earlier than a time we already trusted.
	pub clock_warning: bool,
}

impl CatalogState {
	pub fn freshness(&self, now: OffsetDateTime) -> Option<Freshness> {
		self.catalog.as_ref().map(|catalog| catalog.freshness(now))
	}
}

/// The two downloaded files, unverified.
pub struct RawFeed {
	pub manifest: Vec<u8>,
	pub envelope: Vec<u8>,
}

pub struct CatalogService {
	config: CatalogConfig,
	database: Arc<Database>,
	http: HttpClient,
	state: watch::Sender<Arc<CatalogState>>,
}

impl CatalogService {
	pub(crate) fn new(config: CatalogConfig, database: Arc<Database>, http: HttpClient) -> Self {
		let (state, _) = watch::channel(Arc::new(CatalogState::default()));

		Self {
			config,
			database,
			http,
			state,
		}
	}

	pub fn config(&self) -> &CatalogConfig {
		&self.config
	}

	/// Watches the catalog state. The interface reads it each frame without
	/// blocking, and repaints when it changes.
	pub fn subscribe(&self) -> watch::Receiver<Arc<CatalogState>> {
		self.state.subscribe()
	}

	pub fn state(&self) -> Arc<CatalogState> {
		self.state.borrow().clone()
	}

	/// Loads the stored catalog. It is verified again: the database is not a
	/// trust anchor.
	pub async fn load_cached(&self) {
		self.load_cached_at(OffsetDateTime::now_utc()).await;
	}

	pub(crate) async fn load_cached_at(&self, now: OffsetDateTime) {
		let stored = match self.database.catalog_trust(&self.config.feed_id).await {
			Ok(stored) => stored,
			Err(error) => {
				tracing::warn!(%error, "could not read the stored catalog");
				return self.fail(storage_problem());
			}
		};

		let Some(stored) = stored else {
			return;
		};

		let clock_warning = clock_rolled_back(now, stored.last_seen_at);
		let accepted_at = from_unix(stored.accepted_at);
		let outcome = self
			.verify_blocking(
				stored.manifest,
				stored.envelope,
				now,
				None,
				FreshnessPolicy::AllowExpired,
			)
			.await;

		match outcome {
			Ok(verification) => self.publish(CatalogState {
				catalog: Some(Arc::new(verification.catalog)),
				origin: Some(Origin::Cache { accepted_at }),
				problem: None,
				refreshing: false,
				clock_warning,
			}),
			Err(error) => {
				tracing::warn!(%error, "the stored catalog no longer verifies");
				self.fail(problem_from(&error));
			}
		}
	}

	/// Fetches and verifies the latest catalog. On any failure the previous
	/// verified catalog stays in place and the problem is recorded.
	pub async fn refresh(&self) {
		if self.config.registry.active_key_count() == 0 {
			return self.fail(Problem {
				kind: ProblemKind::NoTrustedKeys,
				code: "CATALOG_KEYS_NOT_CONFIGURED",
				message: "This build has no catalog signing keys configured, so it cannot \
				          verify a catalog yet."
					.into(),
			});
		}

		self.state.send_modify(|state| {
			let mut next = (**state).clone();
			next.refreshing = true;
			*state = Arc::new(next);
		});

		let mut last_problem = None;

		for _ in 0..PAIR_ATTEMPTS {
			let raw = match self.fetch().await {
				Ok(raw) => raw,
				Err(problem) => {
					last_problem = Some(problem);
					break;
				}
			};

			match self.accept(raw, OffsetDateTime::now_utc()).await {
				Ok(()) => return,
				Err(failure) => {
					let retry = failure.retry_with_new_download;
					last_problem = Some(failure.problem);

					if !retry {
						break;
					}
				}
			}
		}

		if let Some(problem) = last_problem {
			self.fail(problem);
		}
	}

	async fn fetch(&self) -> Result<RawFeed, Problem> {
		let (manifest, envelope) = tokio::join!(
			self.http
				.get_bounded(self.config.manifest_url.clone(), limits::MAX_MANIFEST_BYTES),
			self.http
				.get_bounded(self.config.envelope_url.clone(), limits::MAX_ENVELOPE_BYTES),
		);

		match (manifest, envelope) {
			(Ok(manifest), Ok(envelope)) => Ok(RawFeed { manifest, envelope }),
			(Err(error), _) | (_, Err(error)) => {
				tracing::warn!(%error, "catalog download failed");

				Err(Problem {
					kind: ProblemKind::Network,
					code: "RELEASE_LOOKUP_FAILED",
					message:
						"The catalog could not be downloaded. Check your connection and try again."
							.into(),
				})
			}
		}
	}

	/// Verifies `raw` against the stored high-water mark and, if it passes,
	/// persists it and publishes it.
	pub(crate) async fn accept(
		&self,
		raw: RawFeed,
		now: OffsetDateTime,
	) -> Result<(), AcceptFailure> {
		let stored = self
			.database
			.catalog_trust(&self.config.feed_id)
			.await
			.map_err(|error| {
				tracing::warn!(%error, "could not read the catalog trust state");

				AcceptFailure::new(storage_problem(), false)
			})?;

		let clock_warning = stored
			.as_ref()
			.is_some_and(|stored| clock_rolled_back(now, stored.last_seen_at));
		let high_water = stored.as_ref().map(|stored| HighWater {
			revision: stored.revision,
			digest: Some(stored.digest),
		});

		let verification = self
			.verify_blocking(
				raw.manifest,
				raw.envelope,
				now,
				high_water,
				FreshnessPolicy::RequireCurrent,
			)
			.await
			.map_err(|error| {
				tracing::warn!(%error, "catalog verification failed");

				// A mismatch can be a CDN serving mixed generations.
				let retry = matches!(
					error.kind(),
					ErrorKind::SignatureInvalid
						| ErrorKind::HashMismatch
						| ErrorKind::EnvelopeInvalid
				);

				AcceptFailure::new(problem_from(&error), retry)
			})?;

		let now_unix = now.unix_timestamp();

		match verification.change {
			Change::New => {
				let catalog = &verification.catalog;

				self.database
					.accept_catalog(StoredCatalog {
						feed_id: catalog.feed_id().to_owned(),
						revision: catalog.revision(),
						digest: catalog.digest(),
						manifest: catalog.manifest_bytes().to_vec(),
						envelope: catalog.envelope_bytes().to_vec(),
						accepted_at: now_unix,
						last_seen_at: now_unix,
					})
					.await
			}
			Change::Unchanged => {
				self.database
					.note_catalog_check(&self.config.feed_id, now_unix)
					.await
			}
		}
		.map_err(|error| {
			tracing::warn!(%error, "could not persist the verified catalog");

			AcceptFailure::new(storage_problem(), false)
		})?;

		self.publish(CatalogState {
			catalog: Some(Arc::new(verification.catalog)),
			origin: Some(Origin::Network { at: now }),
			problem: None,
			refreshing: false,
			clock_warning,
		});

		Ok(())
	}

	/// Runs verification on the blocking pool. It is CPU-bound and fans out
	/// with rayon, so it must not occupy an async worker.
	async fn verify_blocking(
		&self,
		manifest: Vec<u8>,
		envelope: Vec<u8>,
		now: OffsetDateTime,
		high_water: Option<HighWater>,
		freshness: FreshnessPolicy,
	) -> Result<launcher_catalog::VerifyOutcome, CatalogError> {
		let config = self.config.clone();

		tokio::task::spawn_blocking(move || {
			let context = VerifyContext {
				registry: &config.registry,
				feed_id: &config.feed_id,
				now,
				high_water,
				supported_capabilities: config.supported_capabilities,
				freshness,
			};

			verify(&manifest, &envelope, &context)
		})
		.await
		.unwrap_or_else(|join_error| {
			tracing::error!(%join_error, "catalog verification task failed");

			Err(CatalogError::internal(
				"the verification task did not finish",
			))
		})
	}

	fn publish(&self, state: CatalogState) {
		self.state.send_replace(Arc::new(state));
	}

	/// Records a failed refresh while keeping any verified catalog.
	fn fail(&self, problem: Problem) {
		self.state.send_modify(|state| {
			let mut next = (**state).clone();
			next.problem = Some(problem);
			next.refreshing = false;
			*state = Arc::new(next);
		});
	}
}

pub(crate) struct AcceptFailure {
	pub(crate) problem: Problem,
	pub(crate) retry_with_new_download: bool,
}

impl AcceptFailure {
	fn new(problem: Problem, retry_with_new_download: bool) -> Self {
		Self {
			problem,
			retry_with_new_download,
		}
	}
}

fn storage_problem() -> Problem {
	Problem {
		kind: ProblemKind::Storage,
		code: "DATA_INTEGRITY_FAILURE",
		message: "The launcher's local database could not be read, so the catalog was not used."
			.into(),
	}
}

fn clock_rolled_back(now: OffsetDateTime, last_seen_unix: i64) -> bool {
	now.unix_timestamp() + CLOCK_ROLLBACK_TOLERANCE_SECONDS < last_seen_unix
}

fn from_unix(seconds: i64) -> OffsetDateTime {
	OffsetDateTime::from_unix_timestamp(seconds).unwrap_or(OffsetDateTime::UNIX_EPOCH)
}

/// Maps a verification failure to wording that separates "the network is
/// down" from "the bytes were rejected".
fn problem_from(error: &CatalogError) -> Problem {
	let (kind, message) = match error.kind() {
		ErrorKind::NoTrustedSignature | ErrorKind::SignatureInvalid | ErrorKind::DuplicateKeyId => {
			(
				ProblemKind::Security,
				"The catalog's signature could not be verified, so it was rejected. \
			 Your previous verified catalog is still in use.",
			)
		}
		ErrorKind::HashMismatch | ErrorKind::WrongFeed => (
			ProblemKind::Security,
			"The catalog did not match what its signature covers, so it was rejected.",
		),
		ErrorKind::Rollback | ErrorKind::RevisionConflict => (
			ProblemKind::Security,
			"The server offered an older or conflicting catalog than one already verified. \
			 It was ignored.",
		),
		ErrorKind::Expired => (
			ProblemKind::Unusable,
			"The catalog has expired and the publisher has not renewed it yet.",
		),
		ErrorKind::FutureIssued => (
			ProblemKind::Unusable,
			"The catalog is dated in the future. Check that your computer's clock is correct.",
		),
		ErrorKind::UnsupportedCapability => (
			ProblemKind::Unusable,
			"The catalog needs a feature this version of the launcher does not have. \
			 Update the launcher.",
		),
		_ => (
			ProblemKind::Unusable,
			"The catalog was authentic but could not be understood, so it was not used.",
		),
	};

	Problem {
		kind,
		code: contract_code(error.kind()),
		message: message.to_owned(),
	}
}

/// The stable code scripts see for a verification failure.
fn contract_code(kind: ErrorKind) -> &'static str {
	match kind {
		ErrorKind::NoTrustedSignature
		| ErrorKind::SignatureInvalid
		| ErrorKind::DuplicateKeyId
		| ErrorKind::EnvelopeInvalid
		| ErrorKind::HashMismatch
		| ErrorKind::WrongFeed => "CATALOG_SIGNATURE_INVALID",
		ErrorKind::Expired => "CATALOG_EXPIRED",
		ErrorKind::Rollback | ErrorKind::RevisionConflict => "REVISION_ROLLBACK",
		ErrorKind::UnsupportedCapability => "UNSUPPORTED_SCHEMA",
		ErrorKind::FutureIssued => "CLOCK_UNCERTAIN",
		_ => "CATALOG_INVALID",
	}
}

#[cfg(test)]
mod tests {
	use std::path::Path;

	use launcher_catalog::KeyRegistry;
	use launcher_net::Url;

	use super::*;

	fn fixture(relative: &str) -> Vec<u8> {
		let root = Path::new(env!("CARGO_MANIFEST_DIR"))
			.join("../../domain/launcher-catalog/tests/fixtures");

		std::fs::read(root.join(relative)).unwrap()
	}

	fn test_config() -> CatalogConfig {
		let registry = KeyRegistry::from_test_fixture(
			&String::from_utf8(fixture("test-public-key-registry.json")).unwrap(),
		)
		.unwrap();
		let url = Url::parse("https://example.invalid/x").unwrap();

		CatalogConfig {
			feed_id: "test.remade.launcher-catalog".into(),
			manifest_url: url.clone(),
			envelope_url: url,
			registry: Arc::new(registry),
			supported_capabilities: &["safe-archive-v1"],
		}
	}

	async fn service() -> (tempfile::TempDir, CatalogService) {
		let directory = tempfile::tempdir().unwrap();
		let database = Arc::new(
			Database::open(directory.path().join("db.sqlite"))
				.await
				.unwrap(),
		);
		let service = CatalogService::new(test_config(), database, HttpClient::new().unwrap());

		(directory, service)
	}

	fn raw(case: &str) -> RawFeed {
		RawFeed {
			manifest: fixture(&format!("cases/{case}/catalog.json")),
			envelope: fixture(&format!("cases/{case}/catalog.sig")),
		}
	}

	fn at(text: &str) -> OffsetDateTime {
		OffsetDateTime::parse(text, &time::format_description::well_known::Rfc3339).unwrap()
	}

	const NOW: &str = "2030-01-15T00:00:00Z";

	#[tokio::test]
	async fn accepting_a_valid_catalog_publishes_and_persists_it() {
		let (_directory, service) = service().await;

		assert!(
			service
				.accept(raw("valid-new-revision"), at(NOW))
				.await
				.is_ok()
		);

		let state = service.state();
		assert_eq!(state.catalog.as_ref().unwrap().project_count(), 1);
		assert!(matches!(state.origin, Some(Origin::Network { .. })));

		let stored = service
			.database
			.catalog_trust("test.remade.launcher-catalog")
			.await
			.unwrap();
		assert_eq!(stored.unwrap().revision, 10);
	}

	#[tokio::test]
	async fn a_restart_reverifies_the_stored_catalog() {
		let (_directory, service) = service().await;
		service
			.accept(raw("valid-new-revision"), at(NOW))
			.await
			.ok()
			.unwrap();

		let restarted = CatalogService::new(
			test_config(),
			Arc::clone(&service.database),
			HttpClient::new().unwrap(),
		);
		restarted.load_cached_at(at(NOW)).await;

		let state = restarted.state();
		assert!(state.catalog.is_some());
		assert!(matches!(state.origin, Some(Origin::Cache { .. })));
	}

	#[tokio::test]
	async fn a_tampered_catalog_is_rejected_and_the_good_one_survives() {
		let (_directory, service) = service().await;
		service
			.accept(raw("valid-new-revision"), at(NOW))
			.await
			.ok()
			.unwrap();

		let failure = service
			.accept(raw("altered-bytes"), at(NOW))
			.await
			.err()
			.unwrap();

		assert_eq!(failure.problem.kind, ProblemKind::Security);
		assert!(failure.retry_with_new_download);
		assert_eq!(
			service
				.database
				.catalog_trust("test.remade.launcher-catalog")
				.await
				.unwrap()
				.unwrap()
				.revision,
			10
		);
	}

	#[tokio::test]
	async fn a_rollback_is_rejected_after_a_newer_revision_was_accepted() {
		let (_directory, service) = service().await;
		service
			.accept(raw("valid-new-revision"), at(NOW))
			.await
			.ok()
			.unwrap();

		let failure = service
			.accept(raw("rollback"), at(NOW))
			.await
			.err()
			.unwrap();

		assert_eq!(failure.problem.kind, ProblemKind::Security);
		assert!(!failure.retry_with_new_download);
	}

	#[tokio::test]
	async fn an_expired_catalog_is_refused_from_the_network_but_usable_from_cache() {
		let (_directory, service) = service().await;
		service
			.accept(raw("valid-new-revision"), at(NOW))
			.await
			.ok()
			.unwrap();

		let later = at("2030-03-01T00:00:00Z");
		let failure = service
			.accept(raw("valid-new-revision"), later)
			.await
			.err()
			.unwrap();
		assert_eq!(failure.problem.kind, ProblemKind::Unusable);

		let restarted = CatalogService::new(
			test_config(),
			Arc::clone(&service.database),
			HttpClient::new().unwrap(),
		);
		restarted.load_cached_at(at(NOW)).await;
		assert!(restarted.state().catalog.is_some());
	}

	#[tokio::test]
	async fn the_clock_warning_appears_when_time_moves_backwards() {
		let (_directory, service) = service().await;
		service
			.accept(raw("valid-new-revision"), at("2030-01-20T00:00:00Z"))
			.await
			.ok()
			.unwrap();

		service
			.accept(raw("valid-identical-revision"), at("2030-01-05T00:00:00Z"))
			.await
			.ok()
			.unwrap();

		assert!(service.state().clock_warning);
	}

	#[tokio::test]
	async fn without_trusted_keys_refresh_explains_instead_of_downloading() {
		let directory = tempfile::tempdir().unwrap();
		let database = Arc::new(
			Database::open(directory.path().join("db.sqlite"))
				.await
				.unwrap(),
		);
		let service = CatalogService::new(
			CatalogConfig::production(),
			database,
			HttpClient::new().unwrap(),
		);

		service.refresh().await;

		let problem = service.state().problem.clone().unwrap();
		assert_eq!(problem.kind, ProblemKind::NoTrustedKeys);
	}
}
