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
	let digest = if with_digest {
		format!("\"sha256:{}\"", hex::encode(Sha256::digest(bytes)))
	} else {
		"null".into()
	};

	format!(
		r#"{{"id":{id},"tag_name":"{tag}","draft":false,"prerelease":false,"published_at":"2030-01-0{id}T00:00:00Z",
		"html_url":"https://github.com/fixture-owner/fixture-editor/releases/tag/{tag}","assets":[{{
			"id":{id}0,"name":"sample-editor-{version}-linux-x64.tar.gz","size":{size},"content_type":"application/gzip","state":"uploaded",
			"digest":{digest},"browser_download_url":"{url}"}}]}}"#,
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
}

impl Transport for FakeGithub {
	async fn get(
		&self,
		url: Url,
		_: MetadataRequest,
		_: usize,
	) -> Result<HttpResponse, LauncherError> {
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
	};

	let config = EngineConfig {
		catalog: CatalogConfig {
			feed_id: "test.remade.launcher-catalog".into(),
			manifest_url: url.clone(),
			envelope_url: url,
			registry: Arc::new(registry),
			supported_capabilities: &["safe-archive-v1"],
		},
		artifacts: Some(artifacts.clone()),
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
