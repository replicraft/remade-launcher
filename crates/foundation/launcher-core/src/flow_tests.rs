//! The whole story, from a verified catalog to an installed app, with only
//! GitHub and the artifact bytes faked.

use std::collections::HashMap;
use std::future::Future;
use std::path::Path;
use std::pin::Pin;
use std::sync::{Arc, Mutex};

use launcher_catalog::KeyRegistry;
use launcher_model::{Arch, ErrorCode, HostTarget, LauncherError, Os};
use launcher_net::{Downloaded, HttpResponse, MetadataRequest, Url};
use launcher_providers::{GithubProvider, Transport};
use launcher_resolver::Consent;
use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;

use crate::catalog::RawFeed;
use crate::config::{CatalogConfig, EngineConfig};
use crate::lifecycle::ArtifactSource;
use crate::prepare::{Preparation, custom_plan};
use crate::{DataDirectory, Engine};

const LINUX: HostTarget = HostTarget {
	os: Os::Linux,
	arch: Arch::X86_64,
};
const NOW: &str = "2030-01-15T00:00:00Z";
const REPO: &str = r#"{"id":100001,"name":"fixture-editor","private":false,"owner":{"id":100002,"login":"fixture-owner"}}"#;

fn fixtures() -> std::path::PathBuf {
	Path::new(env!("CARGO_MANIFEST_DIR")).join("../../domain/launcher-catalog/tests/fixtures")
}

fn at(text: &str) -> time::OffsetDateTime {
	time::OffsetDateTime::parse(text, &time::format_description::well_known::Rfc3339).unwrap()
}

fn tarball(version: &str) -> Vec<u8> {
	let encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
	let mut builder = tar::Builder::new(encoder);
	let script = format!("#!/bin/sh\necho {version}\n");

	for (name, data, mode) in [
		(
			format!("sample-editor-{version}/bin/sample-editor"),
			script.as_bytes(),
			0o755,
		),
		(
			format!("sample-editor-{version}/README"),
			b"readme".as_slice(),
			0o644,
		),
	] {
		let mut header = tar::Header::new_gnu();
		header.set_size(data.len() as u64);
		header.set_mode(mode);
		header.set_cksum();
		builder.append_data(&mut header, name, data).unwrap();
	}

	builder.into_inner().unwrap().finish().unwrap()
}

fn asset_url(tag: &str) -> String {
	format!(
		"https://github.com/fixture-owner/fixture-editor/releases/download/{tag}/sample-editor-{}-linux-x64.tar.gz",
		&tag[1..]
	)
}

fn release_json(id: u64, tag: &str, bytes: &[u8], with_digest: bool) -> String {
	release_json_at(
		id,
		tag,
		bytes,
		with_digest,
		&format!("2030-01-0{id}T00:00:00Z"),
	)
}

fn release_json_at(id: u64, tag: &str, bytes: &[u8], with_digest: bool, published: &str) -> String {
	let digest = if with_digest {
		format!("\"sha256:{}\"", hex::encode(Sha256::digest(bytes)))
	} else {
		"null".into()
	};

	format!(
		r#"{{"id":{id},"tag_name":"{tag}","draft":false,"prerelease":false,"published_at":"{published}",
		"html_url":"https://github.com/fixture-owner/fixture-editor/releases/tag/{tag}","assets":[{{
			"id":{id}0,"name":"sample-editor-{version}-linux-x64.tar.gz","size":{size},"content_type":"application/gzip","state":"uploaded",
			"digest":{digest},"created_at":"{published}","updated_at":"{published}","browser_download_url":"{url}"}}]}}"#,
		version = &tag[1..],
		size = bytes.len(),
		url = asset_url(tag),
	)
}

/// A scripted GitHub.
#[derive(Clone)]
struct FakeGithub {
	repo: String,
	releases: Arc<Mutex<String>>,
	/// While true, GitHub answers every request with a server error.
	down: Arc<Mutex<bool>>,
	/// While true, GitHub refuses every request because the limit is reached.
	limited: Arc<Mutex<bool>>,
	/// How many requests it was asked.
	asked: Arc<Mutex<usize>>,
}

impl Transport for FakeGithub {
	async fn get(
		&self,
		url: Url,
		_: MetadataRequest,
		_: usize,
	) -> Result<HttpResponse, LauncherError> {
		*self.asked.lock().unwrap() += 1;

		if *self.limited.lock().unwrap() {
			return Ok(HttpResponse {
				status: 403,
				etag: None,
				retry_after_seconds: None,
				rate_limit_remaining: Some(0),
				rate_limit_reset: None,
				next_page: None,
				body: Vec::new(),
			});
		}

		if *self.down.lock().unwrap() {
			return Ok(HttpResponse {
				status: 500,
				etag: None,
				retry_after_seconds: None,
				rate_limit_remaining: None,
				rate_limit_reset: None,
				next_page: None,
				body: Vec::new(),
			});
		}

		let body = match url.path() {
			"/repositories/100001" | "/repos/fixture-owner/fixture-editor" => self.repo.clone(),
			"/repositories/100001/releases" => self.releases.lock().unwrap().clone(),
			other => panic!("unexpected request: {other}"),
		};

		Ok(HttpResponse {
			status: 200,
			etag: None,
			retry_after_seconds: None,
			rate_limit_remaining: None,
			rate_limit_reset: None,
			next_page: None,
			body: body.into_bytes(),
		})
	}
}

struct Artifacts(Mutex<HashMap<String, Vec<u8>>>);

impl ArtifactSource for Artifacts {
	fn fetch<'a>(
		&'a self,
		url: Url,
		file: &'a mut tokio::fs::File,
		_: u64,
		_: &'a mut (dyn FnMut(u64, Option<u64>) + Send),
	) -> Pin<Box<dyn Future<Output = Result<Downloaded, LauncherError>> + Send + 'a>> {
		Box::pin(async move {
			let bytes = self
				.0
				.lock()
				.unwrap()
				.get(url.as_str())
				.cloned()
				.expect("an unexpected download");

			file.write_all(&bytes).await.unwrap();
			// The real client flushes before it returns, and so must a stand-in.
			file.flush().await.unwrap();

			Ok(Downloaded {
				size: bytes.len() as u64,
				sha256: hex::encode(Sha256::digest(&bytes)),
			})
		})
	}
}

struct World {
	_directory: tempfile::TempDir,
	engine: Engine,
	github: FakeGithub,
	artifacts: Arc<Artifacts>,
}

async fn world() -> World {
	let directory = tempfile::tempdir().unwrap();
	let registry = KeyRegistry::from_test_fixture(
		&std::fs::read_to_string(fixtures().join("test-public-key-registry.json")).unwrap(),
	)
	.unwrap();
	let url = Url::parse("https://example.invalid/x").unwrap();

	let v1 = tarball("1.0.0");
	let artifacts = Arc::new(Artifacts(Mutex::new(HashMap::from([(
		asset_url("v1.0.0"),
		v1.clone(),
	)]))));
	let github = FakeGithub {
		repo: REPO.into(),
		releases: Arc::new(Mutex::new(format!(
			"[{}]",
			release_json(1, "v1.0.0", &v1, true)
		))),
		down: Arc::new(Mutex::new(false)),
		limited: Arc::new(Mutex::new(false)),
		asked: Arc::new(Mutex::new(0)),
	};

	let config = EngineConfig {
		catalog: CatalogConfig {
			feed_id: "test.remade.launcher-catalog".into(),
			manifest_url: url.clone(),
			envelope_url: url,
			registry: Arc::new(registry),
			supported_capabilities: &["safe-archive-v1"],
			updates_feed: None,
		},
		artifacts: Some(artifacts.clone()),
		secrets: Some(Arc::new(crate::MemorySecret::default())),
		releases: crate::ReleaseConfig::production(),
	};

	let engine = Engine::open_with(&DataDirectory::at(directory.path().join("data")), config)
		.await
		.unwrap();

	engine
		.catalog()
		.accept(
			RawFeed {
				manifest: std::fs::read(fixtures().join("valid/launcher-catalog.json")).unwrap(),
				envelope: std::fs::read(fixtures().join("valid/launcher-catalog.sig")).unwrap(),
			},
			at(NOW),
		)
		.await
		.ok()
		.expect("the fixture catalog verifies");

	World {
		_directory: directory,
		engine,
		github,
		artifacts,
	}
}

fn consent(display: &str, acknowledged: bool) -> Consent {
	Consent {
		source_shown: display.into(),
		acknowledged_unreviewed: acknowledged,
	}
}

async fn prepare(world: &World, slug: &str) -> Result<Preparation, LauncherError> {
	let provider = GithubProvider::new(world.github.clone());
	let now = at(NOW).unix_timestamp();

	world
		.engine
		.prepare_curated(&provider, slug, None, Some(LINUX), now)
		.await
}

fn ready(preparation: Preparation) -> crate::Ready {
	match preparation {
		Preparation::Ready(ready) => *ready,
		other => panic!("expected a ready plan, got {other:?}"),
	}
}

fn unavailable(preparation: Preparation) -> String {
	match preparation {
		Preparation::Unavailable(why) => why,
		other => panic!("expected unavailable, got {other:?}"),
	}
}

#[tokio::test]
async fn a_catalog_app_installs_end_to_end_and_appears_in_the_library() {
	let world = world().await;

	let ready = ready(prepare(&world, "sample-editor").await.unwrap());

	assert_eq!(ready.source_display, "fixture-owner/fixture-editor");
	assert!(!ready.unreviewed);
	assert_eq!(ready.candidate.release.tag, "v1.0.0");

	let id = world
		.engine
		.install(&ready, &consent(&ready.source_display, false))
		.await
		.unwrap();
	let installed = world.engine.installed().await.unwrap();

	assert_eq!(installed.len(), 1);
	assert_eq!(installed[0].id, id);
	assert!(installed[0].curated);
	assert!(installed[0].launchable);
	assert_eq!(installed[0].version, "v1.0.0");
	assert_eq!(installed[0].evidence, "host-digest");

	// `strip_components: 1` removed the archive's top-level folder.
	assert!(
		std::path::Path::new(&installed[0].root)
			.join("bin/sample-editor")
			.exists()
	);
}

#[tokio::test]
async fn the_consent_must_name_the_source_the_plan_binds() {
	let world = world().await;
	let ready = ready(prepare(&world, "sample-editor").await.unwrap());

	let error = world
		.engine
		.install(&ready, &consent("someone/else", false))
		.await
		.unwrap_err();

	assert_eq!(error.code(), ErrorCode::PackageUnsupported);
	assert!(world.engine.installed().await.unwrap().is_empty());
}

#[tokio::test]
async fn a_repository_that_no_longer_matches_the_catalog_is_refused() {
	let world = world().await;
	let provider = GithubProvider::new(FakeGithub {
		repo: REPO.replace("\"id\":100002", "\"id\":999999"),
		releases: Arc::new(Mutex::new("[]".into())),
		down: Arc::new(Mutex::new(false)),
		limited: Arc::new(Mutex::new(false)),
		asked: Arc::new(Mutex::new(0)),
	});

	let error = world
		.engine
		.prepare_curated(
			&provider,
			"sample-editor",
			None,
			Some(LINUX),
			at(NOW).unix_timestamp(),
		)
		.await
		.unwrap_err();

	assert_eq!(error.code(), ErrorCode::SourceIdentityChanged);
}

#[tokio::test]
async fn a_missing_host_checksum_stops_the_install_with_a_reason() {
	let world = world().await;
	let v1 = tarball("1.0.0");
	*world.github.releases.lock().unwrap() = format!("[{}]", release_json(1, "v1.0.0", &v1, false));

	let why = unavailable(prepare(&world, "sample-editor").await.unwrap());

	assert!(why.contains("checksum"), "{why}");
}

#[tokio::test]
async fn unknown_apps_and_other_platforms_explain_themselves() {
	let world = world().await;
	let provider = GithubProvider::new(world.github.clone());
	let now = at(NOW).unix_timestamp();

	assert!(unavailable(prepare(&world, "nope").await.unwrap()).contains("not in the catalog"));

	let mac = HostTarget {
		os: Os::Macos,
		arch: Arch::Aarch64,
	};
	let why = unavailable(
		world
			.engine
			.prepare_curated(&provider, "sample-editor", None, Some(mac), now)
			.await
			.unwrap(),
	);

	assert!(why.contains("no package for this device"), "{why}");
}

#[tokio::test]
async fn an_expired_catalog_holds_new_installs() {
	let world = world().await;
	let provider = GithubProvider::new(world.github.clone());
	let later = at("2030-03-01T00:00:00Z").unix_timestamp();

	let why = unavailable(
		world
			.engine
			.prepare_curated(&provider, "sample-editor", None, Some(LINUX), later)
			.await
			.unwrap(),
	);

	assert!(why.contains("expired"), "{why}");
}

#[tokio::test]
async fn updates_keep_the_package_type_and_report_when_current() {
	let world = world().await;
	let first = ready(prepare(&world, "sample-editor").await.unwrap());
	let id = world
		.engine
		.install(&first, &consent(&first.source_display, false))
		.await
		.unwrap();

	let provider = GithubProvider::new(world.github.clone());
	let now = at(NOW).unix_timestamp();

	let why = unavailable(
		world
			.engine
			.prepare_update_with(&provider, id, Some(LINUX), now)
			.await
			.unwrap(),
	);
	assert!(why.contains("up to date"), "{why}");

	// A newer release appears.
	let (v1, v2) = (tarball("1.0.0"), tarball("1.1.0"));
	world
		.artifacts
		.0
		.lock()
		.unwrap()
		.insert(asset_url("v1.1.0"), v2.clone());
	*world.github.releases.lock().unwrap() = format!(
		"[{},{}]",
		release_json(2, "v1.1.0", &v2, true),
		release_json(1, "v1.0.0", &v1, true)
	);

	let update = ready(
		world
			.engine
			.prepare_update_with(&provider, id, Some(LINUX), now)
			.await
			.unwrap(),
	);
	assert_eq!(update.candidate.release.tag, "v1.1.0");
	assert_eq!(
		update.plan.body().operation,
		launcher_resolver::Operation::Update
	);

	world
		.engine
		.update(id, &update, &consent(&update.source_display, false))
		.await
		.unwrap();

	let installed = world.engine.installed().await.unwrap();
	assert_eq!(
		(installed.len(), installed[0].version.as_str()),
		(1, "v1.1.0")
	);
}

#[tokio::test]
async fn a_repository_the_user_added_installs_only_after_acknowledgement() {
	let world = world().await;
	let provider = GithubProvider::new(world.github.clone());

	let prepared = world
		.engine
		.prepare_custom_with(
			&provider,
			"https://github.com/fixture-owner/fixture-editor",
			Some(LINUX),
		)
		.await
		.unwrap();

	assert_eq!(prepared.tag, "v1.0.0");
	assert_eq!(prepared.options.len(), 1);
	assert!(prepared.options[0].managed);

	let ready = custom_plan(
		&prepared,
		prepared.options[0].asset.id,
		at(NOW).unix_timestamp(),
	)
	.unwrap();
	assert!(ready.unreviewed);

	let refused = world
		.engine
		.install(&ready, &consent(&ready.source_display, false))
		.await
		.unwrap_err();
	assert_eq!(refused.code(), ErrorCode::PackageUnsupported);
	assert!(world.engine.installed().await.unwrap().is_empty());

	world
		.engine
		.install(&ready, &consent(&ready.source_display, true))
		.await
		.unwrap();

	let installed = world.engine.installed().await.unwrap();
	assert_eq!(installed.len(), 1);
	assert!(
		!installed[0].curated,
		"a user-added source never becomes curated by matching a name"
	);
}

#[tokio::test]
async fn bad_source_links_are_refused_before_any_request() {
	let world = world().await;
	let provider = GithubProvider::new(world.github.clone());

	for url in [
		"https://github.com.evil.example/o/r",
		"http://github.com/o/r",
		"https://github.com/o",
	] {
		let error = world
			.engine
			.prepare_custom_with(&provider, url, Some(LINUX))
			.await
			.unwrap_err();

		assert_eq!(error.code(), ErrorCode::InvalidSourceUrl, "{url}");
	}
}

#[tokio::test]
async fn an_installed_app_stays_listed_and_removable_when_the_catalog_is_gone() {
	let world = world().await;
	let ready = ready(prepare(&world, "sample-editor").await.unwrap());
	let id = world
		.engine
		.install(&ready, &consent(&ready.source_display, false))
		.await
		.unwrap();

	// A fresh engine over the same data, with a build that trusts no catalog
	// keys at all, as if the app had been delisted and the cache cleared.
	let data = DataDirectory::at(world._directory.path().join("data"));
	let bare = Engine::open_with(&data, EngineConfig::production())
		.await
		.unwrap();

	assert!(bare.catalog().state().catalog.is_none());

	let apps = bare.installed().await.unwrap();
	assert_eq!(
		apps.len(),
		1,
		"the Library comes from receipts, not from the catalog"
	);
	assert_eq!(apps[0].id, id);

	let now = at(NOW).unix_timestamp();
	let report = bare.lifecycle().uninstall(id, now).await.unwrap();

	assert!(report.is_clean());
	assert!(bare.installed().await.unwrap().is_empty());
}

// ---------------------------------------------------------------------------
// Automatic updates
// ---------------------------------------------------------------------------

use launcher_model::{InstallationId, MinimumAge, UpdateMode};
use launcher_storage::AppPolicy;

use crate::updates::{
	CycleResult, Decision, LaunchOutcome, ModeConsent, SkipReason, Trigger, UpdateState,
};

const MATURE: &str = "2030-01-01T00:00:00Z";

fn consent_to_automatic() -> ModeConsent {
	ModeConsent {
		actor: "test".into(),
		acknowledged_risks: true,
	}
}

/// An installed v1.0.0, with GitHub now offering v1.1.0 published at
/// `published`.
async fn world_with_update_published(published: &str) -> (World, InstallationId) {
	let world = world().await;
	let first = ready(prepare(&world, "sample-editor").await.unwrap());
	let id = world
		.engine
		.install(&first, &consent(&first.source_display, false))
		.await
		.unwrap();

	offer(&world, "v1.1.0", 2, published, &tarball("1.1.0"));

	(world, id)
}

/// Makes GitHub list v1.0.0 and a release `tag`, with `bytes` as its file.
fn offer(world: &World, tag: &str, id: u64, published: &str, bytes: &[u8]) {
	world
		.artifacts
		.0
		.lock()
		.unwrap()
		.insert(asset_url(tag), bytes.to_vec());
	*world.github.releases.lock().unwrap() = format!(
		"[{},{}]",
		release_json_at(id, tag, bytes, true, published),
		release_json(1, "v1.0.0", &tarball("1.0.0"), true)
	);
}

async fn cycle(world: &World, trigger: Trigger, now: &str) -> crate::updates::CycleReport {
	let provider = GithubProvider::new(world.github.clone());

	world
		.engine
		.run_update_cycle_with(&provider, trigger, Some(LINUX), at(now).unix_timestamp())
		.await
		.unwrap()
}

async fn statuses(world: &World, now: &str) -> Vec<crate::updates::UpdateStatus> {
	let provider = GithubProvider::new(world.github.clone());

	world
		.engine
		.check_updates_with(&provider, None, Some(LINUX), at(now).unix_timestamp())
		.await
		.unwrap()
}

async fn installed_version(world: &World) -> String {
	world.engine.installed().await.unwrap()[0].version.clone()
}

fn only_skip(report: &crate::updates::CycleReport) -> &SkipReason {
	match &report.outcomes[0].result {
		CycleResult::Skipped(reason) => reason,
		other => panic!("expected a skip, got {other:?}"),
	}
}

#[tokio::test]
async fn checking_for_updates_reports_but_never_installs() {
	let (world, _) = world_with_update_published(MATURE).await;

	let found = statuses(&world, NOW).await;

	assert!(matches!(found[0].state, UpdateState::Available));
	assert_eq!(found[0].available.as_ref().unwrap().version, "v1.1.0");
	assert_eq!(installed_version(&world).await, "v1.0.0");
	assert_eq!(
		found[0].auto.decision,
		Decision::Skip(SkipReason::ModeManual)
	);
}

#[tokio::test]
async fn manual_mode_blocks_every_automatic_trigger() {
	let (world, _) = world_with_update_published(MATURE).await;

	for trigger in [Trigger::OnLaunch, Trigger::Daemon] {
		let report = cycle(&world, trigger, NOW).await;

		assert!(report.mode_blocked && report.outcomes.is_empty());
	}

	assert_eq!(installed_version(&world).await, "v1.0.0");
}

#[tokio::test]
async fn on_launch_mode_installs_a_mature_update_and_records_why() {
	let (world, id) = world_with_update_published(MATURE).await;
	world
		.engine
		.set_update_mode(UpdateMode::OnLaunch, &consent_to_automatic())
		.await
		.unwrap();

	let report = cycle(&world, Trigger::OnLaunch, NOW).await;

	assert_eq!(report.updated(), 1);
	assert_eq!(installed_version(&world).await, "v1.1.0");

	let decisions = world.engine.database.recent_decisions(id, 5).await.unwrap();
	assert_eq!(decisions[0].outcome, "applied");
	assert_eq!(decisions[0].trigger, "on-launch");

	let approvals = world
		.engine
		.database
		.consents("plan", &hex::encode(decisions[0].plan_digest.unwrap()))
		.await
		.unwrap();
	assert_eq!(approvals.len(), 1, "the automatic approval is recorded");
	assert_eq!(approvals[0].actor, "automatic:on-launch");
}

#[tokio::test]
async fn a_release_younger_than_the_minimum_age_waits_then_installs() {
	// Published thirty minutes before NOW, with the one-hour default.
	let (world, id) = world_with_update_published("2030-01-14T23:30:00Z").await;
	world
		.engine
		.set_update_mode(UpdateMode::OnLaunch, &consent_to_automatic())
		.await
		.unwrap();

	let early = cycle(&world, Trigger::OnLaunch, NOW).await;
	assert!(matches!(
		only_skip(&early),
		SkipReason::MinReleaseAgeNotMet { .. }
	));
	assert_eq!(installed_version(&world).await, "v1.0.0");

	// The status says when it becomes eligible.
	let waiting = statuses(&world, NOW).await;
	assert_eq!(
		waiting[0].auto.eligible_at,
		Some(at("2030-01-15T00:30:00Z").unix_timestamp())
	);

	// A person can still update by hand, since the age only governs automatic updates.
	let provider = GithubProvider::new(world.github.clone());
	let manual = ready(
		world
			.engine
			.prepare_update_with(&provider, id, Some(LINUX), at(NOW).unix_timestamp())
			.await
			.unwrap(),
	);
	assert_eq!(manual.candidate.release.tag, "v1.1.0");

	let later = cycle(&world, Trigger::OnLaunch, "2030-01-15T00:31:00Z").await;
	assert_eq!(later.updated(), 1);
	assert_eq!(installed_version(&world).await, "v1.1.0");
}

#[tokio::test]
async fn a_per_app_age_overrides_the_global_one() {
	let (world, id) = world_with_update_published("2030-01-14T22:00:00Z").await;
	world
		.engine
		.set_update_mode(UpdateMode::OnLaunch, &consent_to_automatic())
		.await
		.unwrap();

	// Three hours old: fine for the one-hour default, too young for a day.
	world
		.engine
		.set_app_update_policy(
			id,
			AppPolicy {
				min_age_seconds: Some(86_400),
				..AppPolicy::default()
			},
		)
		.await
		.unwrap();
	assert!(matches!(
		only_skip(&cycle(&world, Trigger::OnLaunch, NOW).await),
		SkipReason::MinReleaseAgeNotMet { .. }
	));

	let status = statuses(&world, NOW).await;
	assert!(status[0].auto.age_is_per_app);
	assert_eq!(
		status[0].auto.minimum_age,
		MinimumAge::from_seconds(86_400).unwrap()
	);

	// A visible, deliberate zero delay.
	world
		.engine
		.set_app_update_policy(
			id,
			AppPolicy {
				min_age_seconds: Some(0),
				..AppPolicy::default()
			},
		)
		.await
		.unwrap();
	assert_eq!(cycle(&world, Trigger::OnLaunch, NOW).await.updated(), 1);
}

#[tokio::test]
async fn a_replaced_file_restarts_the_clock_even_on_an_old_release() {
	let (world, _) = world_with_update_published(MATURE).await;
	world
		.engine
		.set_update_mode(UpdateMode::OnLaunch, &consent_to_automatic())
		.await
		.unwrap();

	// First look: old and unchanged, so eligible. Do not apply yet.
	let first = statuses(&world, NOW).await;
	assert_eq!(first[0].auto.decision, Decision::Apply);

	// Same release and asset ID, different bytes: someone swapped the file.
	offer(&world, "v1.1.0", 2, MATURE, &tarball("1.1.0-swapped"));

	let report = cycle(&world, Trigger::OnLaunch, "2030-01-15T00:10:00Z").await;

	assert!(
		matches!(only_skip(&report), SkipReason::MinReleaseAgeNotMet { .. }),
		"the release's old age must not carry over to a swapped file"
	);
	assert_eq!(installed_version(&world).await, "v1.0.0");
}

#[tokio::test]
async fn pinned_and_paused_apps_are_held_and_an_operator_run_respects_only_the_pin() {
	let (world, id) = world_with_update_published(MATURE).await;
	world
		.engine
		.set_update_mode(UpdateMode::OnLaunch, &consent_to_automatic())
		.await
		.unwrap();

	world.engine.pin(id).await.unwrap();
	assert_eq!(
		*only_skip(&cycle(&world, Trigger::OnLaunch, NOW).await),
		SkipReason::Pinned
	);
	assert_eq!(
		*only_skip(&cycle(&world, Trigger::Operator, NOW).await),
		SkipReason::Pinned
	);
	assert!(matches!(
		statuses(&world, NOW).await[0].state,
		UpdateState::Pinned
	));

	world.engine.unpin(id).await.unwrap();
	world
		.engine
		.set_app_update_policy(
			id,
			AppPolicy {
				auto_paused: true,
				..AppPolicy::default()
			},
		)
		.await
		.unwrap();
	assert_eq!(
		*only_skip(&cycle(&world, Trigger::OnLaunch, NOW).await),
		SkipReason::AutoPaused
	);

	assert_eq!(
		cycle(&world, Trigger::Operator, NOW).await.updated(),
		1,
		"a person's own run is not held by a pause"
	);
}

#[tokio::test]
async fn an_app_can_update_at_launch_while_the_global_mode_is_manual() {
	let (world, id) = world_with_update_published(MATURE).await;

	world
		.engine
		.set_app_update_mode(id, Some(UpdateMode::OnLaunch), &consent_to_automatic())
		.await
		.unwrap();

	assert!(!cycle(&world, Trigger::OnLaunch, NOW).await.mode_blocked);
	assert_eq!(installed_version(&world).await, "v1.1.0");
}

#[tokio::test]
async fn an_app_set_to_manual_is_left_alone_by_an_automatic_global_mode() {
	let (world, id) = world_with_update_published(MATURE).await;
	world
		.engine
		.set_update_mode(UpdateMode::OnLaunch, &consent_to_automatic())
		.await
		.unwrap();
	world
		.engine
		.set_app_update_mode(id, Some(UpdateMode::Manual), &consent_to_automatic())
		.await
		.unwrap();

	assert!(cycle(&world, Trigger::OnLaunch, NOW).await.mode_blocked);
	assert_eq!(installed_version(&world).await, "v1.0.0");

	world
		.engine
		.set_app_update_mode(id, None, &consent_to_automatic())
		.await
		.unwrap();

	assert_eq!(cycle(&world, Trigger::OnLaunch, NOW).await.updated(), 1);
}

#[tokio::test]
async fn the_background_updater_only_does_what_asked_for_it() {
	let (world, id) = world_with_update_published(MATURE).await;
	world
		.engine
		.set_update_mode(UpdateMode::Daemon, &consent_to_automatic())
		.await
		.unwrap();
	world
		.engine
		.set_app_update_mode(id, Some(UpdateMode::OnLaunch), &consent_to_automatic())
		.await
		.unwrap();

	assert!(cycle(&world, Trigger::Daemon, NOW).await.mode_blocked);
	assert_eq!(installed_version(&world).await, "v1.0.0");
	assert_eq!(cycle(&world, Trigger::OnLaunch, NOW).await.updated(), 1);
}

#[tokio::test]
async fn an_app_cannot_use_the_background_updater_unless_all_apps_do() {
	let (world, id) = world_with_update_published(MATURE).await;
	world
		.engine
		.set_app_update_mode(id, Some(UpdateMode::Daemon), &consent_to_automatic())
		.await
		.unwrap();

	assert!(cycle(&world, Trigger::Daemon, NOW).await.mode_blocked);
	assert_eq!(cycle(&world, Trigger::OnLaunch, NOW).await.updated(), 1);
}

#[tokio::test]
async fn pinned_is_a_level_for_all_apps_that_one_app_can_step_out_of() {
	let (world, id) = world_with_update_published(MATURE).await;
	world
		.engine
		.set_update_mode(UpdateMode::Pinned, &consent_to_automatic())
		.await
		.unwrap();

	assert!(matches!(
		statuses(&world, NOW).await[0].state,
		UpdateState::Pinned
	));
	assert_eq!(
		*only_skip(&cycle(&world, Trigger::Operator, NOW).await),
		SkipReason::Pinned
	);

	world
		.engine
		.set_app_update_mode(id, Some(UpdateMode::Manual), &consent_to_automatic())
		.await
		.unwrap();

	assert!(matches!(
		statuses(&world, NOW).await[0].state,
		UpdateState::Available
	));
	assert_eq!(cycle(&world, Trigger::Operator, NOW).await.updated(), 1);
}

#[tokio::test]
async fn choosing_pinned_for_an_app_holds_it_and_choosing_again_releases_it() {
	let (world, id) = world_with_update_published(MATURE).await;
	let nothing = ModeConsent {
		actor: "test".into(),
		acknowledged_risks: false,
	};

	world
		.engine
		.set_app_update_mode(id, Some(UpdateMode::Pinned), &nothing)
		.await
		.unwrap();
	assert!(
		world
			.engine
			.app_update_policy(id)
			.await
			.unwrap()
			.pinned_release_id
			.is_some()
	);

	world
		.engine
		.set_app_update_mode(id, Some(UpdateMode::Manual), &nothing)
		.await
		.unwrap();

	let policy = world.engine.app_update_policy(id).await.unwrap();

	assert_eq!(policy.pinned_release_id, None);
	assert_eq!(policy.update_mode, Some(UpdateMode::Manual));
}

#[tokio::test]
async fn an_automatic_level_for_one_app_needs_the_acknowledgement_too() {
	let (world, id) = world_with_update_published(MATURE).await;
	let refuse = ModeConsent {
		actor: "test".into(),
		acknowledged_risks: false,
	};

	let error = world
		.engine
		.set_app_update_mode(id, Some(UpdateMode::OnLaunch), &refuse)
		.await
		.unwrap_err();

	assert_eq!(error.code(), ErrorCode::ConsentRequired);
	assert_eq!(
		world
			.engine
			.app_update_policy(id)
			.await
			.unwrap()
			.update_mode,
		None
	);
}

#[tokio::test]
async fn a_rebuilt_file_with_the_same_version_is_never_applied_unattended() {
	let (world, _) = world_with_update_published(MATURE).await;
	world
		.engine
		.set_update_mode(UpdateMode::OnLaunch, &consent_to_automatic())
		.await
		.unwrap();

	// v1.0.0 is republished with a different file (a different asset ID).
	let rebuilt = tarball("1.0.0-rebuilt");
	world
		.artifacts
		.0
		.lock()
		.unwrap()
		.insert(asset_url("v1.0.0"), rebuilt.clone());
	let json = release_json(1, "v1.0.0", &rebuilt, true).replace("\"id\":10,", "\"id\":11,");
	*world.github.releases.lock().unwrap() = format!("[{json}]");

	let report = cycle(&world, Trigger::OnLaunch, NOW).await;

	assert_eq!(*only_skip(&report), SkipReason::RebuiltArtifact);

	let found = statuses(&world, NOW).await;
	assert!(
		found[0].available.as_ref().unwrap().rebuilt,
		"it is still shown to a person"
	);
}

#[tokio::test]
async fn a_failed_lookup_is_never_reported_as_up_to_date() {
	let (world, _) = world_with_update_published(MATURE).await;
	*world.github.down.lock().unwrap() = true;

	let found = statuses(&world, NOW).await;

	assert!(
		matches!(found[0].state, UpdateState::CheckFailed(_)),
		"{:?}",
		found[0].state
	);
}

#[tokio::test]
async fn a_catalog_that_has_expired_holds_automatic_updates() {
	let (world, _) = world_with_update_published(MATURE).await;
	world
		.engine
		.set_update_mode(UpdateMode::OnLaunch, &consent_to_automatic())
		.await
		.unwrap();

	let report = cycle(&world, Trigger::OnLaunch, "2030-03-01T00:00:00Z").await;

	assert!(matches!(only_skip(&report), SkipReason::Unavailable(why) if why.contains("expired")));
	assert_eq!(installed_version(&world).await, "v1.0.0");
}

#[tokio::test]
async fn apps_the_user_added_themselves_are_not_updated_unattended() {
	let world = world().await;
	let provider = GithubProvider::new(world.github.clone());
	let prepared = world
		.engine
		.prepare_custom_with(
			&provider,
			"https://github.com/fixture-owner/fixture-editor",
			Some(LINUX),
		)
		.await
		.unwrap();
	let ready = custom_plan(
		&prepared,
		prepared.options[0].asset.id,
		at(NOW).unix_timestamp(),
	)
	.unwrap();
	world
		.engine
		.install(&ready, &consent(&ready.source_display, true))
		.await
		.unwrap();

	world
		.engine
		.set_update_mode(UpdateMode::OnLaunch, &consent_to_automatic())
		.await
		.unwrap();

	let report = cycle(&world, Trigger::OnLaunch, NOW).await;

	assert_eq!(*only_skip(&report), SkipReason::SourceNotEnrolled);
}

#[tokio::test]
async fn changing_the_mode_starts_nothing_and_a_launch_acts_once() {
	let (world, _) = world_with_update_published(MATURE).await;

	// Switching modes is only a setting.
	world
		.engine
		.set_update_mode(UpdateMode::OnLaunch, &consent_to_automatic())
		.await
		.unwrap();
	assert_eq!(installed_version(&world).await, "v1.0.0");
	assert!(
		world
			.engine
			.database
			.setting("updates.last_cycle_started")
			.await
			.unwrap()
			.is_none()
	);

	// A genuine launcher start, in on-launch mode, runs one cycle.
	let provider = GithubProvider::new(world.github.clone());
	let _ = &provider;
	let first = world.engine.launcher_started().await.unwrap();
	assert!(matches!(first, LaunchOutcome::Cycle(_)));

	// Anything else in the same process (a focus change, a second window) is not a start.
	assert!(matches!(
		world.engine.launcher_started().await.unwrap(),
		LaunchOutcome::AlreadyHandled
	));
	assert!(matches!(
		world.engine.launcher_started().await.unwrap(),
		LaunchOutcome::AlreadyHandled
	));
}

#[tokio::test]
async fn a_second_cycle_does_nothing_while_one_is_running_elsewhere() {
	let (world, _) = world_with_update_published(MATURE).await;
	world
		.engine
		.set_update_mode(UpdateMode::OnLaunch, &consent_to_automatic())
		.await
		.unwrap();

	let held = crate::lifecycle::cycle_lock(&world.engine.data).expect("the lock is free");
	let report = cycle(&world, Trigger::OnLaunch, NOW).await;
	drop(held);

	assert!(report.already_running && report.outcomes.is_empty());
	assert_eq!(installed_version(&world).await, "v1.0.0");
	assert_eq!(cycle(&world, Trigger::OnLaunch, NOW).await.updated(), 1);
}

#[tokio::test]
async fn prerelease_opt_in_is_per_app_and_off_by_default() {
	let (world, id) = world_with_update_published(MATURE).await;
	world
		.engine
		.set_update_mode(UpdateMode::OnLaunch, &consent_to_automatic())
		.await
		.unwrap();

	let beta = tarball("2.0.0-rc.1");
	world
		.artifacts
		.0
		.lock()
		.unwrap()
		.insert(asset_url("v2.0.0-rc.1"), beta.clone());
	let rc = release_json_at(3, "v2.0.0-rc.1", &beta, true, MATURE)
		.replace("\"prerelease\":false", "\"prerelease\":true");
	let stable = release_json_at(2, "v1.1.0", &tarball("1.1.0"), true, MATURE);
	*world.github.releases.lock().unwrap() = format!(
		"[{rc},{stable},{}]",
		release_json(1, "v1.0.0", &tarball("1.0.0"), true)
	);

	let found = statuses(&world, NOW).await;
	assert_eq!(found[0].available.as_ref().unwrap().version, "v1.1.0");

	world
		.engine
		.set_app_update_policy(
			id,
			AppPolicy {
				allow_prerelease: true,
				..AppPolicy::default()
			},
		)
		.await
		.unwrap();

	let found = statuses(&world, NOW).await;
	assert_eq!(found[0].available.as_ref().unwrap().version, "v2.0.0-rc.1");
}

// ---------------------------------------------------------------------------
// Saved plans
// ---------------------------------------------------------------------------

use launcher_storage::StoredPlan;

use crate::saved_plans::{Applied, Approval, SavedPlan};

fn approval(confirmed: bool, accepted: Option<&str>) -> Approval {
	Approval {
		actor: "test".into(),
		confirmed,
		accepted_unreviewed_source: accepted.map(str::to_owned),
	}
}

#[tokio::test]
async fn a_saved_plan_is_applied_exactly_as_planned() {
	let world = world().await;
	let ready = ready(prepare(&world, "sample-editor").await.unwrap());
	let id = world.engine.save_ready_plan(&ready, None).await.unwrap();

	assert_eq!(id.len(), 16);

	let SavedPlan::Install { ready: loaded, .. } = world
		.engine
		.load_plan_at(&id[..6], at(NOW).unix_timestamp())
		.await
		.unwrap()
	else {
		panic!("expected an install plan");
	};
	assert_eq!(loaded.plan.digest(), ready.plan.digest());
	assert_eq!(loaded.candidate.asset.name, ready.candidate.asset.name);

	let applied = world
		.engine
		.apply_plan_at(&id, &approval(true, None), at(NOW).unix_timestamp())
		.await
		.unwrap();

	assert!(matches!(applied, Applied::Installed(_)));
	assert_eq!(world.engine.installed().await.unwrap().len(), 1);

	let approvals = world
		.engine
		.database
		.consents("plan", &hex::encode(ready.plan.digest()))
		.await
		.unwrap();
	assert_eq!(
		approvals.len(),
		1,
		"the approval is recorded against the plan's digest"
	);
}

#[tokio::test]
async fn an_unconfirmed_plan_changes_nothing() {
	let world = world().await;
	let ready = ready(prepare(&world, "sample-editor").await.unwrap());
	let id = world.engine.save_ready_plan(&ready, None).await.unwrap();

	let error = world
		.engine
		.apply_plan_at(&id, &approval(false, None), at(NOW).unix_timestamp())
		.await
		.unwrap_err();

	assert_eq!(error.code(), ErrorCode::ConfirmationRequired);
	assert!(world.engine.installed().await.unwrap().is_empty());
}

#[tokio::test]
async fn an_unreviewed_source_must_be_accepted_by_name() {
	let world = world().await;
	let provider = GithubProvider::new(world.github.clone());
	let prepared = world
		.engine
		.prepare_custom_with(
			&provider,
			"https://github.com/fixture-owner/fixture-editor",
			Some(LINUX),
		)
		.await
		.unwrap();
	let ready = custom_plan(
		&prepared,
		prepared.options[0].asset.id,
		at(NOW).unix_timestamp(),
	)
	.unwrap();
	let id = world.engine.save_ready_plan(&ready, None).await.unwrap();
	let now = at(NOW).unix_timestamp();

	for accepted in [None, Some("someone/else"), Some("fixture-owner/other")] {
		let error = world
			.engine
			.apply_plan_at(&id, &approval(true, accepted), now)
			.await
			.unwrap_err();

		assert_eq!(
			error.code(),
			ErrorCode::SourceApprovalRequired,
			"{accepted:?}"
		);
	}

	assert!(world.engine.installed().await.unwrap().is_empty());

	world
		.engine
		.apply_plan_at(
			&id,
			&approval(true, Some("fixture-owner/fixture-editor")),
			now,
		)
		.await
		.unwrap();

	assert_eq!(world.engine.installed().await.unwrap().len(), 1);
}

#[tokio::test]
async fn an_expired_or_edited_plan_is_refused() {
	let world = world().await;
	let ready = ready(prepare(&world, "sample-editor").await.unwrap());
	let id = world.engine.save_ready_plan(&ready, None).await.unwrap();

	let late = at("2030-01-15T01:00:00Z").unix_timestamp();
	let expired = world
		.engine
		.apply_plan_at(&id, &approval(true, None), late)
		.await
		.unwrap_err();
	assert_eq!(expired.code(), ErrorCode::PlanExpired);

	// A row whose body no longer matches its recorded digest.
	let body = serde_json::to_string(ready.plan.body()).unwrap();
	world
		.engine
		.database
		.save_plan(StoredPlan {
			id: "forged0000000000".into(),
			digest: [9; 32],
			body,
			extra: "{}".into(),
			update_of: None,
			created_at: 0,
			expires_at: i64::MAX,
		})
		.await
		.unwrap();

	let forged = world.engine.load_plan_at("forged", 0).await.unwrap_err();
	assert_eq!(forged.code(), ErrorCode::PlanChanged);

	let missing = world.engine.load_plan_at("nothere", 0).await.unwrap_err();
	assert_eq!(missing.code(), ErrorCode::PlanChanged);
}

#[tokio::test]
async fn an_update_plan_is_void_once_the_app_has_moved_on() {
	let (world, id) = world_with_update_published(MATURE).await;
	let provider = GithubProvider::new(world.github.clone());
	let now = at(NOW).unix_timestamp();

	let update = ready(
		world
			.engine
			.prepare_update_with(&provider, id, Some(LINUX), now)
			.await
			.unwrap(),
	);
	let plan_id = world
		.engine
		.save_ready_plan(&update, Some(id))
		.await
		.unwrap();

	// Someone updates it another way first.
	world
		.engine
		.update(id, &update, &consent(&update.source_display, false))
		.await
		.unwrap();

	let error = world
		.engine
		.apply_plan_at(&plan_id, &approval(true, None), now)
		.await
		.unwrap_err();

	assert_eq!(error.code(), ErrorCode::PlanChanged);
}

#[tokio::test]
async fn a_pinned_app_refuses_a_saved_update_plan() {
	let (world, id) = world_with_update_published(MATURE).await;
	let provider = GithubProvider::new(world.github.clone());
	let now = at(NOW).unix_timestamp();

	let update = ready(
		world
			.engine
			.prepare_update_with(&provider, id, Some(LINUX), now)
			.await
			.unwrap(),
	);
	let plan_id = world
		.engine
		.save_ready_plan(&update, Some(id))
		.await
		.unwrap();
	world.engine.pin(id).await.unwrap();

	let error = world
		.engine
		.apply_plan_at(&plan_id, &approval(true, None), now)
		.await
		.unwrap_err();

	assert_eq!(error.code(), ErrorCode::AppPinned);
	assert_eq!(installed_version(&world).await, "v1.0.0");
}

#[tokio::test]
async fn an_uninstall_plan_shows_what_it_will_do_and_then_does_it() {
	let world = world().await;
	let ready = ready(prepare(&world, "sample-editor").await.unwrap());
	let id = world
		.engine
		.install(&ready, &consent(&ready.source_display, false))
		.await
		.unwrap();

	let plan = world
		.engine
		.plan_uninstall_at(id, at(NOW).unix_timestamp())
		.await
		.unwrap();
	assert_eq!(plan.name, "Sample Editor");
	assert!(plan.owned_files >= 3);

	let now = at(NOW).unix_timestamp();
	let unconfirmed = world
		.engine
		.apply_plan_at(&plan.id, &approval(false, None), now)
		.await
		.unwrap_err();
	assert_eq!(unconfirmed.code(), ErrorCode::ConfirmationRequired);
	assert_eq!(world.engine.installed().await.unwrap().len(), 1);

	let applied = world
		.engine
		.apply_plan_at(&plan.id, &approval(true, None), now)
		.await
		.unwrap();

	assert!(matches!(applied, Applied::Removed(report) if report.is_clean()));
	assert!(world.engine.installed().await.unwrap().is_empty());
}

// ---------------------------------------------------------------------------
// Looking things up, and checking what is installed
// ---------------------------------------------------------------------------

async fn installed_world() -> (World, InstallationId, std::path::PathBuf) {
	let world = world().await;
	let ready = ready(prepare(&world, "sample-editor").await.unwrap());
	let id = world
		.engine
		.install(&ready, &consent(&ready.source_display, false))
		.await
		.unwrap();
	let root = std::path::PathBuf::from(&world.engine.installed().await.unwrap()[0].root);

	(world, id, root)
}

#[tokio::test]
async fn verify_passes_on_a_fresh_install_and_names_edits_and_losses() {
	let (world, id, root) = installed_world().await;

	let clean = world.engine.verify_installation(id).await.unwrap();
	assert!(clean.is_clean(), "{clean:?}");
	assert!(clean.files_checked >= 3);

	std::fs::write(root.join("README"), "edited").unwrap();
	std::fs::remove_file(root.join("bin/sample-editor")).unwrap();

	let dirty = world.engine.verify_installation(id).await.unwrap();

	assert!(!dirty.is_clean());
	assert_eq!(dirty.modified, vec!["README".to_owned()]);
	assert_eq!(dirty.missing, vec!["bin/sample-editor".to_owned()]);
	assert!(dirty.ownership_ok);
}

#[tokio::test]
async fn verify_reports_a_folder_that_is_no_longer_the_launchers() {
	let (world, id, root) = installed_world().await;
	std::fs::remove_file(root.join(".remade-install")).unwrap();

	let report = world.engine.verify_installation(id).await.unwrap();

	assert!(!report.ownership_ok && !report.is_clean());
	assert!(
		report.problems[0].contains("not marked"),
		"{:?}",
		report.problems
	);
}

#[tokio::test]
async fn doctor_is_read_only_and_points_at_unfinished_work() {
	let (world, _id, root) = installed_world().await;

	assert!(world.engine.doctor().await.unwrap().is_healthy());

	std::fs::write(root.join("README"), "edited").unwrap();
	let before = std::fs::read_to_string(root.join("README")).unwrap();

	let report = world.engine.doctor().await.unwrap();

	assert!(!report.is_healthy());
	assert_eq!(report.apps[0].modified, vec!["README".to_owned()]);
	assert_eq!(
		std::fs::read_to_string(root.join("README")).unwrap(),
		before,
		"doctor repairs nothing"
	);
}

#[tokio::test]
async fn releases_lists_every_release_with_its_verdict() {
	let world = world().await;
	let provider = GithubProvider::new(world.github.clone());

	let views = world
		.engine
		.releases_with(&provider, "sample-editor", Some(LINUX))
		.await
		.unwrap();

	assert_eq!(views.len(), 1);
	assert_eq!(views[0].tag, "v1.0.0");
	assert_eq!(
		views[0].status,
		launcher_resolver::ReleaseStatus::Eligible { has_package: true }
	);

	let missing = world
		.engine
		.releases_with(&provider, "nope", Some(LINUX))
		.await
		.unwrap_err();
	assert_eq!(missing.code(), ErrorCode::InstallationNotFound);
}

#[tokio::test]
async fn a_changelog_is_plain_text_even_when_the_notes_are_hostile() {
	let world = world().await;
	let provider = GithubProvider::new(world.github.clone());
	let release = release_json(1, "v1.0.0", &tarball("1.0.0"), true).replace(
		"\"assets\"",
		"\"body\":\"Fixes <script>x()</script>\\u001b[2Jdone\",\"assets\"",
	);
	*world.github.releases.lock().unwrap() = format!("[{release}]");

	let notes = world
		.engine
		.changelog_with(&provider, "sample-editor", None)
		.await
		.unwrap();

	assert_eq!(notes.version, "v1.0.0");
	assert_eq!(notes.text, "Fixes x()[2Jdone");
}

#[tokio::test]
async fn platforms_explain_each_package_type_for_this_device() {
	let world = world().await;
	let report = world.engine.platforms(Some("sample-editor")).unwrap();
	let variants = &report.apps[0].1;

	assert_eq!(variants.len(), 1);
	// The fixture builds for Linux x86-64; whatever this test machine is, the
	// answer must carry a reason.
	assert!(!variants[0].reason.is_empty());
	assert_eq!(
		variants[0].supported,
		HostTarget::current().is_some_and(|host| host.os == Os::Linux && host.arch == Arch::X86_64)
	);

	assert!(world.engine.platforms(Some("nope")).is_err());
	assert_eq!(world.engine.platforms(None).unwrap().apps.len(), 1);
}

#[tokio::test]
async fn trust_status_lists_keys_and_explain_says_what_is_checked() {
	let world = world().await;
	let status = world.engine.trust_status();

	assert_eq!(status.feed_id, "test.remade.launcher-catalog");
	assert_eq!(status.trusted_key_ids.len(), 2);
	assert_eq!(status.catalog_revision, Some(10));
	assert!(!status.verified_by.is_empty());

	let lines = world
		.engine
		.explain_trust("sample-editor")
		.unwrap()
		.join("\n");

	assert!(lines.contains("fixture-owner/fixture-editor"));
	assert!(lines.contains("intact, not who made it"));
	assert!(lines.contains("does not mean the software is harmless"));
}

#[tokio::test]
async fn forget_drops_the_record_and_leaves_the_files() {
	let (world, id, root) = installed_world().await;

	world.engine.forget(id).await.unwrap();

	assert!(world.engine.installed().await.unwrap().is_empty());
	assert!(
		root.join("bin/sample-editor").exists(),
		"forget never touches files"
	);

	let history = world.engine.history(1).await.unwrap();
	assert_eq!(history[0].state, "completed");
	assert!(
		history[0]
			.detail
			.as_deref()
			.unwrap()
			.contains("left in place")
	);
}

#[tokio::test]
async fn forget_refuses_a_folder_it_cannot_prove_is_its_own() {
	let (world, id, root) = installed_world().await;
	std::fs::remove_file(root.join(".remade-install")).unwrap();

	let error = world.engine.forget(id).await.unwrap_err();

	assert_eq!(error.code(), ErrorCode::InstallationChanged);
	assert_eq!(world.engine.installed().await.unwrap().len(), 1);
}

#[tokio::test]
async fn switching_to_manual_in_the_middle_of_a_pass_stops_the_next_install() {
	let (world, _) = world_with_update_published(MATURE).await;
	world
		.engine
		.set_update_mode(UpdateMode::OnLaunch, &consent_to_automatic())
		.await
		.unwrap();

	let provider = GithubProvider::new(world.github.clone());
	let now = at(NOW).unix_timestamp();

	// The pass has looked everything up and decided to apply...
	let mut evaluated = world
		.engine
		.evaluate_apps(
			&provider,
			None,
			crate::updates::Initiator::Automatic,
			Some(LINUX),
			now,
		)
		.await
		.unwrap();
	assert_eq!(evaluated[0].decision, Decision::Apply);

	// ...and then the person turns automatic updates off.
	let nothing = ModeConsent {
		actor: "test".into(),
		acknowledged_risks: false,
	};
	world
		.engine
		.set_update_mode(UpdateMode::Manual, &nothing)
		.await
		.unwrap();

	let outcome = world
		.engine
		.settle("cycle", Trigger::OnLaunch, evaluated.remove(0), now)
		.await;

	assert!(matches!(
		outcome.result,
		CycleResult::Skipped(SkipReason::ModeManual)
	));
	assert_eq!(
		installed_version(&world).await,
		"v1.0.0",
		"no install may start after the switch"
	);
}

#[tokio::test]
async fn unreadable_publication_times_never_make_a_young_release_instantly_eligible() {
	let world = world().await;
	let first = ready(prepare(&world, "sample-editor").await.unwrap());
	world
		.engine
		.install(&first, &consent(&first.source_display, false))
		.await
		.unwrap();
	world
		.engine
		.set_update_mode(UpdateMode::OnLaunch, &consent_to_automatic())
		.await
		.unwrap();

	// A new release whose dates are garbage, which the launcher cannot trust.
	let (bytes, tag) = (tarball("1.1.0"), "v1.1.0");
	world
		.artifacts
		.0
		.lock()
		.unwrap()
		.insert(asset_url(tag), bytes.clone());
	let garbage = release_json_at(2, tag, &bytes, true, "not a date");
	*world.github.releases.lock().unwrap() = format!(
		"[{garbage},{}]",
		release_json(1, "v1.0.0", &tarball("1.0.0"), true)
	);

	let report = cycle(&world, Trigger::OnLaunch, NOW).await;

	assert!(
		matches!(only_skip(&report), SkipReason::MinReleaseAgeNotMet { eligible_at } if *eligible_at == at(NOW).unix_timestamp() + 3_600)
	);
	assert_eq!(installed_version(&world).await, "v1.0.0");

	// Once the launcher has itself watched it for the full age, it is eligible.
	let later = cycle(&world, Trigger::OnLaunch, "2030-01-15T01:01:00Z").await;
	assert_eq!(later.updated(), 1);
}

#[tokio::test]
async fn a_repository_that_changed_hands_blocks_automatic_updates() {
	let (world, _) = world_with_update_published(MATURE).await;
	world
		.engine
		.set_update_mode(UpdateMode::OnLaunch, &consent_to_automatic())
		.await
		.unwrap();

	let provider = GithubProvider::new(FakeGithub {
		repo: REPO.replace("\"id\":100002", "\"id\":999999"),
		releases: world.github.releases.clone(),
		down: Arc::new(Mutex::new(false)),
		limited: Arc::new(Mutex::new(false)),
		asked: Arc::new(Mutex::new(0)),
	});

	let report = world
		.engine
		.run_update_cycle_with(
			&provider,
			Trigger::OnLaunch,
			Some(LINUX),
			at(NOW).unix_timestamp(),
		)
		.await
		.unwrap();

	assert!(
		matches!(only_skip(&report), SkipReason::CheckFailed(why) if why.contains("no longer matches"))
	);
	assert_eq!(installed_version(&world).await, "v1.0.0");
}

// ---------------------------------------------------------------------------
// Repair
// ---------------------------------------------------------------------------

async fn assess(world: &World, id: InstallationId) -> crate::RepairAssessment {
	let provider = GithubProvider::new(world.github.clone());

	world
		.engine
		.assess_repair_with(&provider, id, Some(LINUX), at(NOW).unix_timestamp())
		.await
		.unwrap()
}

#[tokio::test]
async fn a_healthy_app_needs_no_repair() {
	let (world, id, _root) = installed_world().await;

	let assessment = assess(&world, id).await;

	assert!(matches!(
		assessment.outcome,
		crate::RepairOutcome::NotNeeded
	));
	assert!(assessment.report.is_clean());
}

#[tokio::test]
async fn a_damaged_app_is_put_back_and_edits_are_kept() {
	let (world, id, root) = installed_world().await;

	std::fs::remove_file(root.join("bin/sample-editor")).unwrap();
	std::fs::write(root.join("notes.txt"), "mine").unwrap();

	let assessment = assess(&world, id).await;
	let crate::RepairOutcome::Ready(ready) = assessment.outcome else {
		panic!("a plan was expected");
	};

	assert_eq!(
		assessment.report.missing,
		vec!["bin/sample-editor".to_owned()]
	);
	assert!(
		ready.notes.iter().any(|note| note.contains("kept")),
		"{:?}",
		ready.notes
	);

	let plan_id = world
		.engine
		.save_ready_plan(&ready, Some(id))
		.await
		.unwrap();

	world
		.engine
		.apply_plan_at(&plan_id, &approval(true, None), at(NOW).unix_timestamp())
		.await
		.unwrap();

	let after = world.engine.installed().await.unwrap();
	let root = std::path::PathBuf::from(&after[0].root);

	assert!(
		root.join("bin/sample-editor").exists(),
		"the missing file is back"
	);
	assert!(
		world
			.engine
			.verify_installation(after[0].id)
			.await
			.unwrap()
			.is_clean()
	);
}

#[tokio::test]
async fn a_folder_that_is_no_longer_the_launchers_is_not_written_to() {
	let (world, id, root) = installed_world().await;

	std::fs::remove_file(root.join(".remade-install")).unwrap();

	let assessment = assess(&world, id).await;

	let crate::RepairOutcome::Unavailable(why) = assessment.outcome else {
		panic!("a refusal was expected");
	};

	assert!(why.contains("no longer marked"), "{why}");
}

#[tokio::test]
async fn a_pinned_app_can_be_repaired_at_its_own_release_but_not_upgraded_by_it() {
	let (world, id, root) = installed_world().await;

	world.engine.pin(id).await.unwrap();
	std::fs::remove_file(root.join("bin/sample-editor")).unwrap();

	// Same release still current: allowed, and the saved plan applies.
	let assessment = assess(&world, id).await;
	let crate::RepairOutcome::Ready(ready) = assessment.outcome else {
		panic!("a plan was expected");
	};
	let plan_id = world
		.engine
		.save_ready_plan(&ready, Some(id))
		.await
		.unwrap();

	world
		.engine
		.apply_plan_at(&plan_id, &approval(true, None), at(NOW).unix_timestamp())
		.await
		.unwrap();

	// A newer release appears: repair must not upgrade a pinned app.
	let (v1, v2) = (tarball("1.0.0"), tarball("1.1.0"));
	world
		.artifacts
		.0
		.lock()
		.unwrap()
		.insert(asset_url("v1.1.0"), v2.clone());
	*world.github.releases.lock().unwrap() = format!(
		"[{},{}]",
		release_json_at(2, "v1.1.0", &v2, true, MATURE),
		release_json_at(1, "v1.0.0", &v1, true, "2029-01-01T00:00:00Z")
	);

	let root = std::path::PathBuf::from(&world.engine.installed().await.unwrap()[0].root);

	std::fs::remove_file(root.join("bin/sample-editor")).unwrap();

	let blocked = assess(&world, id).await;

	let crate::RepairOutcome::Unavailable(why) = blocked.outcome else {
		panic!("a refusal was expected");
	};

	assert!(why.contains("pinned"), "{why}");
}

// ---------------------------------------------------------------------------
// Download without installing
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_download_is_checked_and_saved_but_nothing_is_installed() {
	let world = world().await;
	let ready = ready(prepare(&world, "sample-editor").await.unwrap());
	let folder = tempfile::tempdir().unwrap();
	let mut seen = 0;

	let saved = world
		.engine
		.download_artifact(&ready, folder.path(), &mut |received, _| seen = received)
		.await
		.unwrap();

	assert!(saved.path.starts_with(folder.path()) && saved.path.exists());
	assert_eq!(saved.size, std::fs::metadata(&saved.path).unwrap().len());
	assert_eq!(
		saved.sha256,
		hex::encode(Sha256::digest(std::fs::read(&saved.path).unwrap()))
	);
	assert!(
		saved.matched_expected_sha256,
		"the host's checksum was known"
	);
	assert!(
		world.engine.installed().await.unwrap().is_empty(),
		"nothing was installed"
	);
	assert!(
		std::fs::read_dir(folder.path())
			.unwrap()
			.flatten()
			.all(|entry| !entry.file_name().to_string_lossy().ends_with(".part")),
		"no partial file is left"
	);
	let _ = seen;
}

#[tokio::test]
async fn an_existing_file_is_never_overwritten() {
	let world = world().await;
	let ready = ready(prepare(&world, "sample-editor").await.unwrap());
	let folder = tempfile::tempdir().unwrap();

	world
		.engine
		.download_artifact(&ready, folder.path(), &mut |_, _| {})
		.await
		.unwrap();

	let error = world
		.engine
		.download_artifact(&ready, folder.path(), &mut |_, _| {})
		.await
		.unwrap_err();

	assert_eq!(error.code(), ErrorCode::InstallationChanged);
}

#[tokio::test]
async fn a_download_that_does_not_match_its_checksum_leaves_nothing() {
	let world = world().await;
	let ready = ready(prepare(&world, "sample-editor").await.unwrap());
	let folder = tempfile::tempdir().unwrap();

	// The server now serves different bytes under the same address.
	world
		.artifacts
		.0
		.lock()
		.unwrap()
		.insert(asset_url("v1.0.0"), tarball("9.9.9"));

	let error = world
		.engine
		.download_artifact(&ready, folder.path(), &mut |_, _| {})
		.await
		.unwrap_err();

	assert_eq!(error.code(), ErrorCode::ArtifactDigestMismatch);
	assert_eq!(
		std::fs::read_dir(folder.path()).unwrap().count(),
		0,
		"neither the file nor a partial one remains"
	);
}

fn observed_release(id: u64, tag: &str, assets: &[u64]) -> launcher_catalog::ObservedRelease {
	launcher_catalog::ObservedRelease {
		id,
		tag: tag.to_owned(),
		prerelease: tag.contains('-'),
		published_at: Some("2030-01-01T00:00:00Z".into()),
		asset_ids: assets.to_vec(),
	}
}

/// What the website's daily note says about the sample app.
fn note_of(
	newest: launcher_catalog::ObservedRelease,
	newest_stable: Option<launcher_catalog::ObservedRelease>,
	owner_id: &str,
	valid_until: &str,
) -> launcher_catalog::VerifiedObservations {
	launcher_catalog::VerifiedObservations::for_test(
		vec![(
			"sample-editor".to_owned(),
			launcher_catalog::ObservedProject {
				repository_id: "100001".into(),
				owner_id: owner_id.into(),
				newest: Some(newest),
				newest_stable,
			},
		)],
		at(valid_until),
	)
}

/// The note for the installed release (ID 1, file 10) being the newest.
fn current_note(owner_id: &str, valid_until: &str) -> launcher_catalog::VerifiedObservations {
	let installed = observed_release(1, "v1.0.0", &[10]);

	note_of(installed.clone(), Some(installed), owner_id, valid_until)
}

#[tokio::test]
async fn a_note_that_says_current_answers_without_asking_github() {
	let (world, _) = world_with_update_published(MATURE).await;

	// GitHub is unreachable, which would normally make the check fail.
	*world.github.down.lock().unwrap() = true;
	world
		.engine
		.observations
		.set_for_test(current_note("100002", "2099-01-01T00:00:00Z"));

	let found = statuses(&world, NOW).await;

	assert!(
		matches!(found[0].state, UpdateState::UpToDate),
		"{:?}",
		found[0].state
	);
}

#[tokio::test]
async fn a_note_never_offers_an_update_so_a_newer_release_is_looked_up_live() {
	let (world, _) = world_with_update_published(MATURE).await;
	let newer = observed_release(2, "v1.1.0", &[20]);

	world.engine.observations.set_for_test(note_of(
		newer.clone(),
		Some(newer),
		"100002",
		"2099-01-01T00:00:00Z",
	));

	// With GitHub down, "an update exists" is not taken from the note.
	*world.github.down.lock().unwrap() = true;

	let found = statuses(&world, NOW).await;

	assert!(
		matches!(found[0].state, UpdateState::CheckFailed(_)),
		"{:?}",
		found[0].state
	);

	// With GitHub up, the offer comes from GitHub's own release.
	*world.github.down.lock().unwrap() = false;

	let found = statuses(&world, NOW).await;

	assert!(matches!(found[0].state, UpdateState::Available));
	assert_eq!(found[0].available.as_ref().unwrap().version, "v1.1.0");
}

#[tokio::test]
async fn a_replaced_file_or_a_doubtful_note_means_a_live_look() {
	let installed = observed_release(1, "v1.0.0", &[10]);
	let replaced = observed_release(1, "v1.0.0", &[11]);

	let notes = [
		// The installed file is no longer in the release.
		note_of(
			replaced.clone(),
			Some(replaced),
			"100002",
			"2099-01-01T00:00:00Z",
		),
		// The note is about another repository than the catalog approved.
		current_note("999", "2099-01-01T00:00:00Z"),
		// The note has expired.
		current_note("100002", "2000-01-01T00:00:00Z"),
		// The note has no stable release to compare with.
		note_of(installed, None, "100002", "2099-01-01T00:00:00Z"),
	];

	for (index, note) in notes.into_iter().enumerate() {
		let (world, _) = world_with_update_published(MATURE).await;

		*world.github.down.lock().unwrap() = true;
		world.engine.observations.set_for_test(note);

		let found = statuses(&world, NOW).await;

		assert!(
			matches!(found[0].state, UpdateState::CheckFailed(_)),
			"case {index}: {:?}",
			found[0].state
		);
	}
}

#[tokio::test]
async fn someone_who_follows_pre_releases_is_not_told_up_to_date_by_the_stable_line() {
	let (world, id) = world_with_update_published(MATURE).await;
	let installed = observed_release(1, "v1.0.0", &[10]);
	let beta = observed_release(3, "v2.0.0-rc.1", &[30]);

	*world.github.down.lock().unwrap() = true;
	world.engine.observations.set_for_test(note_of(
		beta,
		Some(installed),
		"100002",
		"2099-01-01T00:00:00Z",
	));

	// The stable line is current, so the default check is satisfied.
	assert!(matches!(
		statuses(&world, NOW).await[0].state,
		UpdateState::UpToDate
	));

	world
		.engine
		.set_app_update_policy(
			id,
			AppPolicy {
				allow_prerelease: true,
				..AppPolicy::default()
			},
		)
		.await
		.unwrap();

	// A newer pre-release exists, so an opted-in app has to look.
	assert!(matches!(
		statuses(&world, NOW).await[0].state,
		UpdateState::CheckFailed(_)
	));
}

#[tokio::test]
async fn once_github_refuses_for_its_limit_the_checks_wait_instead_of_asking_again() {
	let (world, _) = world_with_update_published(MATURE).await;
	let asked = |world: &World| *world.github.asked.lock().unwrap();

	*world.github.limited.lock().unwrap() = true;

	let first_asked = asked(&world);
	let found = statuses(&world, NOW).await;

	assert!(matches!(found[0].state, UpdateState::CheckFailed(_)));
	assert!(asked(&world) > first_asked, "the first look had to ask");

	// GitHub has not recovered, and nothing should be sent in the meantime.
	let after_first = asked(&world);
	let found = statuses(&world, NOW).await;

	assert_eq!(asked(&world), after_first, "no request while waiting");
	match &found[0].state {
		UpdateState::CheckFailed(why) => assert!(why.contains("GitHub token"), "{why}"),
		other => panic!("{other:?}"),
	}

	// Adding a token clears the wait, so the next check asks again.
	*world.github.limited.lock().unwrap() = false;
	world
		.engine
		.set_github_token("ghp_abcdefghijklmnopqrstuvwxyz0123456789")
		.await
		.unwrap();

	let found = statuses(&world, NOW).await;

	assert!(
		matches!(found[0].state, UpdateState::Available),
		"{:?}",
		found[0].state
	);
}
