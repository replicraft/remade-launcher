use std::collections::HashMap;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::{Arc, Mutex};

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use ed25519_dalek::{Signer, SigningKey};
use launcher_catalog::KeyRegistry;
use launcher_model::{ErrorCode, LauncherError, LauncherUpdates, UpdateMode};
use launcher_net::{Downloaded, Url};
use serde_json::json;
use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;

use super::*;
use crate::lifecycle::ArtifactSource;
use crate::{DataDirectory, EngineConfig, MemorySecret, ModeConsent};

const FEED: &str = "test.remade.launcher-releases";
const NOW: &str = "2030-01-15T00:00:00Z";

fn now() -> OffsetDateTime {
	OffsetDateTime::parse(NOW, &time::format_description::well_known::Rfc3339).unwrap()
}

struct Served(Mutex<HashMap<String, Vec<u8>>>);

impl ArtifactSource for Served {
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
			file.flush().await.unwrap();

			Ok(Downloaded {
				size: bytes.len() as u64,
				sha256: hex::encode(Sha256::digest(&bytes)),
			})
		})
	}
}

/// A release archive the way the release script lays it out: one folder.
fn archive(version: &str, files: &[(&str, &str)]) -> Vec<u8> {
	let encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
	let mut builder = tar::Builder::new(encoder);

	for (name, text) in files {
		let mut header = tar::Header::new_gnu();

		header.set_size(text.len() as u64);
		header.set_mode(0o755);
		header.set_cksum();
		builder
			.append_data(
				&mut header,
				format!("remade-{version}/{name}"),
				text.as_bytes(),
			)
			.unwrap();
	}

	builder.into_inner().unwrap().finish().unwrap()
}

/// The same, with the macOS app bundle beside the programs.
fn archive_with_app(version: &str) -> Vec<u8> {
	let encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
	let mut builder = tar::Builder::new(encoder);

	for (name, text, mode) in [
		("remade", format!("new cli {version}"), 0o755),
		(
			"Remade Launcher.app/Contents/MacOS/remade-launcher",
			format!("new gui {version}"),
			0o755,
		),
		(
			"Remade Launcher.app/Contents/Info.plist",
			format!("new plist {version}"),
			0o644,
		),
	] {
		let mut header = tar::Header::new_gnu();

		header.set_size(text.len() as u64);
		header.set_mode(mode);
		header.set_cksum();
		builder
			.append_data(
				&mut header,
				format!("remade-{version}/{name}"),
				text.as_bytes(),
			)
			.unwrap();
	}

	builder.into_inner().unwrap().finish().unwrap()
}

fn url_for(version: &str) -> String {
	format!(
		"https://github.com/replicraft/remade-launcher/releases/download/v{version}/remade-{version}.tar.gz"
	)
}

struct Setup {
	_directory: tempfile::TempDir,
	engine: Engine,
	install: PathBuf,
	served: Arc<Served>,
	key: SigningKey,
}

fn read(path: &Path) -> String {
	std::fs::read_to_string(path).unwrap()
}

/// How the launcher under test is installed.
enum Shape {
	Programs,
	Managed(String),
	Bundle,
}

async fn setup(current: &str, managed: Option<&str>) -> Setup {
	setup_shape(
		current,
		managed.map_or(Shape::Programs, |by| Shape::Managed(by.to_owned())),
	)
	.await
}

async fn setup_shape(current: &str, shape: Shape) -> Setup {
	let directory = tempfile::tempdir().unwrap();
	let install = directory.path().join("install");

	std::fs::create_dir_all(&install).unwrap();
	std::fs::write(install.join("remade"), "old cli").unwrap();
	std::fs::write(install.join("remade-launcher-daemon"), "old daemon").unwrap();
	std::fs::write(install.join("notes.txt"), "mine").unwrap();

	let old_app = install.join("Remade Launcher.app/Contents");

	std::fs::create_dir_all(old_app.join("MacOS")).unwrap();
	std::fs::write(old_app.join("MacOS/remade-launcher"), "old gui").unwrap();
	std::fs::write(old_app.join("Info.plist"), "old plist").unwrap();

	let key = SigningKey::from_bytes(&[9u8; 32]);
	let public = STANDARD.encode(key.verifying_key().to_bytes());
	let registry = KeyRegistry::from_test_release_fixture(&format!(
		r#"{{"schema_version":1,"feed_id":"{FEED}","test_only":true,"keys":[
			{{"key_id":"rel","algorithm":"ed25519","role":"updater","status":"active","public_key":"{public}"}}]}}"#
	))
	.unwrap();
	let served = Arc::new(Served(Mutex::new(HashMap::new())));
	let layout = match shape {
		Shape::Managed(by) => InstallLayout::Managed { by },
		Shape::Programs => InstallLayout::Standalone {
			directory: install.clone(),
			executable: "remade".into(),
		},
		Shape::Bundle => InstallLayout::AppBundle {
			bundle: install.join("Remade Launcher.app"),
			executable: "remade-launcher".into(),
		},
	};
	let config = EngineConfig {
		artifacts: Some(served.clone()),
		secrets: Some(Arc::new(MemorySecret::default())),
		releases: ReleaseConfig {
			feed_id: FEED.into(),
			registry: Arc::new(registry),
			current_version: current.into(),
			os: "linux",
			arch: "x86_64",
			layout: Some(layout),
			..ReleaseConfig::production()
		},
		..EngineConfig::production()
	};
	let engine = Engine::open_with(&DataDirectory::at(directory.path().join("data")), config)
		.await
		.unwrap();

	Setup {
		_directory: directory,
		engine,
		install,
		served,
		key,
	}
}

impl Setup {
	/// Serves `bytes` as version `version`, and returns the signed document
	/// for it. `claimed` is what the document says the file's bytes hash to.
	fn publish(
		&self,
		version: &str,
		revision: u64,
		bytes: Vec<u8>,
		claimed: &[u8],
	) -> (Vec<u8>, Vec<u8>) {
		self.served
			.0
			.lock()
			.unwrap()
			.insert(url_for(version), bytes);

		let document = serde_json::to_vec(&json!({
			"schema_version": 1, "feed_id": FEED, "channel": "stable", "revision": revision,
			"issued_at": "2030-01-10T00:00:00Z", "expires_at": "2030-02-10T00:00:00Z",
			"version": version,
			"notes_url": format!("https://github.com/replicraft/remade-launcher/releases/tag/v{version}"),
			"artifacts": [{
				"os": "linux", "arch": "x86_64", "name": format!("remade-{version}.tar.gz"),
				"url": url_for(version),
				"sha256": hex::encode(Sha256::digest(claimed)), "size": claimed.len()
			}]
		}))
		.unwrap();
		let signature = STANDARD.encode(self.key.sign(&document).to_bytes());
		let envelope = format!(
			r#"{{"version":1,"signatures":[{{"key_id":"rel","algorithm":"ed25519","signature":"{signature}"}}]}}"#
		);

		(document, envelope.into_bytes())
	}

	async fn release(&self, version: &str, revision: u64) -> launcher_catalog::VerifiedRelease {
		let bytes = archive(
			version,
			&[
				("remade", &format!("new cli {version}")),
				("remade-launcher-daemon", &format!("new daemon {version}")),
				("LICENSE", "text"),
			],
		);
		let (document, envelope) = self.publish(version, revision, bytes.clone(), &bytes);

		self.engine
			.accept_release(&document, &envelope, now())
			.await
			.unwrap()
	}

	async fn release_with_app(
		&self,
		version: &str,
		revision: u64,
	) -> launcher_catalog::VerifiedRelease {
		let bytes = archive_with_app(version);
		let (document, envelope) = self.publish(version, revision, bytes.clone(), &bytes);

		self.engine
			.accept_release(&document, &envelope, now())
			.await
			.unwrap()
	}

	async fn stage(
		&self,
		release: &launcher_catalog::VerifiedRelease,
	) -> Result<StagedLauncher, LauncherError> {
		self.engine.stage_release(release, &mut |_, _| {}).await
	}
}

#[tokio::test]
async fn a_new_launcher_is_staged_then_switched_in_and_can_be_rolled_back() {
	let world = setup("1.0.0", None).await;
	let release = world.release("1.1.0", 1).await;

	let found = world.engine.note_release(&release).await.unwrap();

	assert_eq!(
		found,
		LauncherCheck::Available(LauncherRelease {
			version: "1.1.0".into(),
			notes_url: Some(
				"https://github.com/replicraft/remade-launcher/releases/tag/v1.1.0".into()
			),
			size: Some(world.served.0.lock().unwrap()[&url_for("1.1.0")].len() as u64),
		})
	);

	let staged = world.stage(&release).await.unwrap();

	assert_eq!(staged.version, "1.1.0");
	assert_eq!(
		read(&world.install.join("remade")),
		"old cli",
		"staging changes nothing yet"
	);

	let report = world.engine.launcher_update_report().await;

	assert_eq!(
		report.staged.as_ref().map(|s| s.version.as_str()),
		Some("1.1.0")
	);
	assert!(!report.can_roll_back);

	let applied = world.engine.apply_launcher_update().await.unwrap().unwrap();

	assert_eq!(
		(applied.from.as_str(), applied.to.as_str()),
		("1.0.0", "1.1.0")
	);
	assert_eq!(read(&world.install.join("remade")), "new cli 1.1.0");
	assert_eq!(
		read(&world.install.join("remade-launcher-daemon")),
		"new daemon 1.1.0"
	);
	assert_eq!(read(&world.install.join("notes.txt")), "mine");
	assert!(
		!world.install.join("LICENSE").exists(),
		"only the programs are replaced"
	);
	assert!(world.engine.launcher_update_report().await.can_roll_back);
	assert_eq!(
		world.engine.apply_launcher_update().await.unwrap(),
		None,
		"nothing is left to apply"
	);

	let back = world.engine.roll_back_launcher().await.unwrap();

	assert_eq!(back.to, "1.0.0");
	assert_eq!(read(&world.install.join("remade")), "old cli");
	assert_eq!(
		read(&world.install.join("remade-launcher-daemon")),
		"old daemon"
	);
	assert!(!world.engine.launcher_update_report().await.can_roll_back);
}

#[tokio::test]
async fn a_download_that_is_not_what_was_signed_is_discarded() {
	let world = setup("1.0.0", None).await;
	let signed = archive("1.1.0", &[("remade", "genuine")]);
	let evil = archive("1.1.0", &[("remade", "evil")]);
	let (document, envelope) = world.publish("1.1.0", 1, evil, &signed);
	let release = world
		.engine
		.accept_release(&document, &envelope, now())
		.await
		.unwrap();

	let error = world.stage(&release).await.unwrap_err();

	assert_eq!(error.code(), ErrorCode::ArtifactDigestMismatch);
	assert!(world.engine.launcher_update_report().await.staged.is_none());
	assert!(!world.engine.update_folder().join("staged").exists());
	assert_eq!(read(&world.install.join("remade")), "old cli");
}

#[tokio::test]
async fn an_archive_without_the_running_program_is_refused() {
	let world = setup("1.0.0", None).await;
	let bytes = archive("1.1.0", &[("remade-launcher-daemon", "daemon only")]);
	let (document, envelope) = world.publish("1.1.0", 1, bytes.clone(), &bytes);
	let release = world
		.engine
		.accept_release(&document, &envelope, now())
		.await
		.unwrap();

	assert_eq!(
		world.stage(&release).await.unwrap_err().code(),
		ErrorCode::PackageUnsupported
	);
}

#[tokio::test]
async fn a_launcher_a_package_manager_owns_is_left_alone() {
	let world = setup("1.0.0", Some("Homebrew")).await;
	let release = world.release("1.1.0", 1).await;

	// It can still be told about.
	assert!(matches!(
		world.engine.note_release(&release).await.unwrap(),
		LauncherCheck::Available(_)
	));

	let error = world.stage(&release).await.unwrap_err();

	assert_eq!(error.code(), ErrorCode::SelfUpdateUnavailable);
	assert!(error.user_message().contains("Homebrew"));
	assert_eq!(
		world.engine.launcher_update_report().await.blocker,
		Some(Blocker::ManagedBy("Homebrew".into()))
	);
	assert_eq!(world.engine.apply_launcher_update().await.unwrap(), None);
}

#[tokio::test]
async fn the_same_or_an_older_release_is_never_installed() {
	let world = setup("1.1.0", None).await;
	let same = world.release("1.1.0", 1).await;
	let older = world.release("1.0.0", 2).await;

	assert_eq!(
		world.engine.note_release(&same).await.unwrap(),
		LauncherCheck::UpToDate
	);
	assert_eq!(
		world.engine.note_release(&older).await.unwrap(),
		LauncherCheck::UpToDate
	);
	assert_eq!(
		world.stage(&older).await.unwrap_err().code(),
		ErrorCode::SelfUpdateUnavailable
	);
}

#[tokio::test]
async fn an_older_release_document_cannot_replace_a_newer_one() {
	let world = setup("1.0.0", None).await;

	world.release("1.2.0", 7).await;

	let bytes = archive("1.1.0", &[("remade", "x")]);
	let (document, envelope) = world.publish("1.1.0", 3, bytes.clone(), &bytes);
	let error = world
		.engine
		.accept_release(&document, &envelope, now())
		.await
		.unwrap_err();

	assert_eq!(error.code(), ErrorCode::ReleaseRejected);
}

#[tokio::test]
async fn nothing_is_switched_while_an_install_is_running() {
	let world = setup("1.0.0", None).await;
	let release = world.release("1.1.0", 1).await;

	world.stage(&release).await.unwrap();

	let held = crate::lifecycle::acquire_operation_lock(world.engine.data_directory())
		.await
		.unwrap();
	let error = world.engine.apply_launcher_update().await.unwrap_err();

	assert_eq!(error.code(), ErrorCode::OperationInProgress);
	assert_eq!(read(&world.install.join("remade")), "old cli");

	drop(held);

	assert!(
		world
			.engine
			.apply_launcher_update()
			.await
			.unwrap()
			.is_some()
	);
}

#[tokio::test]
async fn a_staged_file_that_changed_is_not_switched_in() {
	let world = setup("1.0.0", None).await;
	let release = world.release("1.1.0", 1).await;

	world.stage(&release).await.unwrap();
	std::fs::write(
		world
			.engine
			.update_folder()
			.join("staged")
			.join("payload")
			.join("remade"),
		"tampered",
	)
	.unwrap();

	let error = world.engine.apply_launcher_update().await.unwrap_err();

	assert_eq!(error.code(), ErrorCode::ArtifactDigestMismatch);
	assert_eq!(read(&world.install.join("remade")), "old cli");
}

#[tokio::test]
async fn a_staged_update_the_launcher_has_since_caught_up_with_is_dropped() {
	let world = setup("1.0.0", None).await;
	let release = world.release("1.1.0", 1).await;

	world.stage(&release).await.unwrap();

	// The same state, seen by a launcher that is already at 1.1.0.
	let caught_up = setup("1.1.0", None).await;

	caught_up
		.engine
		.set_state_value(
			STAGED_KEY,
			world.engine.state_value(STAGED_KEY).await.unwrap(),
		)
		.await
		.unwrap();

	assert_eq!(
		caught_up.engine.apply_launcher_update().await.unwrap(),
		None
	);
	assert!(
		caught_up
			.engine
			.launcher_update_report()
			.await
			.staged
			.is_none()
	);
}

#[tokio::test]
async fn off_means_no_automatic_look_and_no_cycle() {
	let world = setup("1.0.0", None).await;
	let mut preferences = world.engine.preferences().await;

	preferences.launcher_updates = LauncherUpdates::Off;
	world.engine.set_preferences(preferences).await.unwrap();

	assert_eq!(
		world.engine.check_launcher_update(true).await.unwrap(),
		LauncherCheck::Off
	);
	assert_eq!(
		world.engine.launcher_update_cycle().await,
		LauncherUpdateOutcome::Idle
	);
}

#[tokio::test]
async fn the_launcher_setting_and_the_app_setting_do_not_touch_each_other() {
	let world = setup("1.0.0", None).await;

	assert_eq!(
		world.engine.preferences().await.launcher_updates,
		LauncherUpdates::Automatic
	);
	assert_eq!(
		world.engine.update_settings().await.mode,
		UpdateMode::Manual
	);

	let mut preferences = world.engine.preferences().await;

	preferences.launcher_updates = LauncherUpdates::Notify;
	world.engine.set_preferences(preferences).await.unwrap();

	assert_eq!(
		world.engine.update_settings().await.mode,
		UpdateMode::Manual,
		"changing the launcher's setting leaves app updates alone"
	);

	world
		.engine
		.set_update_mode(
			UpdateMode::OnLaunch,
			&ModeConsent {
				actor: "test".into(),
				acknowledged_risks: true,
			},
		)
		.await
		.unwrap();

	assert_eq!(
		world.engine.preferences().await.launcher_updates,
		LauncherUpdates::Notify,
		"changing app updates leaves the launcher's setting alone"
	);

	world
		.engine
		.set_update_mode(
			UpdateMode::Manual,
			&ModeConsent {
				actor: "test".into(),
				acknowledged_risks: false,
			},
		)
		.await
		.unwrap();
	world.engine.restore_defaults().await.unwrap();

	assert_eq!(
		world.engine.preferences().await.launcher_updates,
		LauncherUpdates::Automatic,
		"restoring defaults goes back to Automatic"
	);
}

#[tokio::test]
async fn this_build_has_no_release_key_so_it_checks_nothing() {
	let directory = tempfile::tempdir().unwrap();
	let no_keys = KeyRegistry::from_test_release_fixture(&format!(
		r#"{{"schema_version":1,"feed_id":"{FEED}","test_only":true,"keys":[]}}"#
	))
	.unwrap();
	let config = EngineConfig {
		secrets: Some(Arc::new(MemorySecret::default())),
		releases: ReleaseConfig {
			feed_id: FEED.into(),
			registry: Arc::new(no_keys),
			..ReleaseConfig::production()
		},
		..EngineConfig::production()
	};
	let engine = Engine::open_with(&DataDirectory::at(directory.path().join("data")), config)
		.await
		.unwrap();

	assert_eq!(
		engine.check_launcher_update(false).await.unwrap(),
		LauncherCheck::Blocked(Blocker::NoKeys)
	);
	assert_eq!(
		engine.launcher_update_cycle().await,
		LauncherUpdateOutcome::Idle
	);
	assert_eq!(
		engine
			.stage_launcher_update(&mut |_, _| {})
			.await
			.unwrap_err()
			.code(),
		ErrorCode::SelfUpdateUnavailable
	);
	assert_eq!(
		engine.launcher_update_report().await.blocker,
		Some(Blocker::NoKeys)
	);
}

#[tokio::test]
async fn a_macos_app_is_replaced_whole_and_can_be_put_back() {
	let world = setup_shape("1.0.0", Shape::Bundle).await;
	let release = world.release_with_app("1.1.0", 1).await;
	let app = world.install.join("Remade Launcher.app/Contents");

	world.stage(&release).await.unwrap();

	assert_eq!(read(&app.join("MacOS/remade-launcher")), "old gui");

	let applied = world.engine.apply_launcher_update().await.unwrap().unwrap();

	assert_eq!(applied.to, "1.1.0");
	assert_eq!(read(&app.join("MacOS/remade-launcher")), "new gui 1.1.0");
	assert_eq!(read(&app.join("Info.plist")), "new plist 1.1.0");
	assert_eq!(
		read(&world.install.join("remade")),
		"old cli",
		"a bundle install replaces its app, and nothing beside it"
	);
	assert!(world.engine.launcher_update_report().await.can_roll_back);

	world.engine.roll_back_launcher().await.unwrap();

	assert_eq!(read(&app.join("MacOS/remade-launcher")), "old gui");
	assert_eq!(read(&app.join("Info.plist")), "old plist");
}

#[tokio::test]
async fn an_archive_without_the_app_does_not_update_a_bundle() {
	let world = setup_shape("1.0.0", Shape::Bundle).await;
	// This archive only has the command-line programs.
	let release = world.release("1.1.0", 1).await;

	assert_eq!(
		world.stage(&release).await.unwrap_err().code(),
		ErrorCode::PackageUnsupported
	);
}

#[tokio::test]
async fn two_programs_cannot_update_the_launcher_at_once() {
	let world = setup("1.0.0", None).await;
	let release = world.release("1.1.0", 1).await;
	let held = UpdateLock::take(world.engine.data_directory()).unwrap();

	assert_eq!(
		world.stage(&release).await.unwrap_err().code(),
		ErrorCode::OperationInProgress
	);

	drop(held);

	assert!(world.stage(&release).await.is_ok());
}
