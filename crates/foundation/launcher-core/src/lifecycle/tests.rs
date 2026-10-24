//! End-to-end lifecycle tests against real files and a real database. The
//! only fake is where the artifact bytes come from.

use std::collections::HashMap;
use std::future::Future;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;

use launcher_model::{ErrorCode, InstallationId, LauncherError, OperationId};
use launcher_net::{Downloaded, Url};
use launcher_resolver::{
	AssetBinding, Consent, EvidenceLevel, InstallBinding, Operation, Origin, Plan, PlanBody,
	ReleaseBinding, SourceBinding,
};
use launcher_storage::{Database, OperationRecord};
use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;

use super::artifact::ArtifactSource;
use super::install::state;
use super::*;
use crate::events::{EngineEvent, EventBus, Stage};
use crate::paths::DataDirectory;

const NOW: i64 = 1_000;
const URL: &str = "https://github.com/o/r/releases/download/v1/app.zip";

/// Serves local files in place of the network.
struct LocalArtifacts(HashMap<String, PathBuf>);

impl ArtifactSource for LocalArtifacts {
	fn fetch<'a>(
		&'a self,
		url: Url,
		file: &'a mut tokio::fs::File,
		_max_bytes: u64,
		_progress: &'a mut (dyn FnMut(u64, Option<u64>) + Send),
	) -> Pin<Box<dyn Future<Output = Result<Downloaded, LauncherError>> + Send + 'a>> {
		Box::pin(async move {
			let source = self.0.get(url.as_str()).expect("an unexpected download");
			let bytes = tokio::fs::read(source).await.unwrap();

			file.write_all(&bytes).await.unwrap();
			file.flush().await.unwrap();

			Ok(Downloaded {
				size: bytes.len() as u64,
				sha256: hex::encode(Sha256::digest(&bytes)),
			})
		})
	}
}

struct Fixture {
	_directory: tempfile::TempDir,
	data: DataDirectory,
	service: LifecycleService,
	database: Arc<Database>,
	events: EventBus,
	archive: PathBuf,
}

fn write_zip(path: &Path, entries: &[(&str, &[u8], u32)]) {
	let mut writer = zip::ZipWriter::new(std::fs::File::create(path).unwrap());

	for (name, data, mode) in entries {
		let options = zip::write::SimpleFileOptions::default().unix_permissions(*mode);

		writer.start_file(*name, options).unwrap();
		writer.write_all(data).unwrap();
	}

	writer.finish().unwrap();
}

const SCRIPT: &[u8] = b"#!/bin/sh\necho launched > ran.txt\n";

async fn fixture_with(entries: &[(&str, &[u8], u32)]) -> Fixture {
	let directory = tempfile::tempdir().unwrap();
	let data = DataDirectory::at(directory.path().join("data"));
	data.ensure_exists().unwrap();

	let archive = directory.path().join("app.zip");
	write_zip(&archive, entries);

	let database = Arc::new(Database::open(data.database_file()).await.unwrap());
	let events = EventBus::new();
	let artifacts = LocalArtifacts(HashMap::from([(URL.to_owned(), archive.clone())]));
	let service = LifecycleService::new(
		data.clone(),
		Arc::clone(&database),
		Arc::new(artifacts),
		events.clone(),
	);

	Fixture {
		_directory: directory,
		data,
		service,
		database,
		events,
		archive,
	}
}

async fn fixture() -> Fixture {
	fixture_with(&[
		("run.sh", SCRIPT, 0o755),
		("data/info.txt", b"hello", 0o644),
	])
	.await
}

fn digest_of(path: &Path) -> String {
	hex::encode(Sha256::digest(std::fs::read(path).unwrap()))
}

fn plan(
	fixture: &Fixture,
	release: u64,
	expected_sha256: Option<String>,
	entry_point: Option<&str>,
) -> launcher_resolver::ApprovedPlan {
	let size = std::fs::metadata(&fixture.archive).unwrap().len();

	let body = PlanBody {
		schema_version: 1,
		operation: Operation::Install,
		origin: Origin::Custom,
		source: SourceBinding {
			repository_id: 1,
			owner_id: 2,
			display: "o/r".into(),
		},
		release: ReleaseBinding {
			id: release,
			tag: format!("v{release}"),
		},
		asset: AssetBinding {
			id: 50,
			name: "app.zip".into(),
			size,
			download_url: URL.into(),
		},
		install: InstallBinding {
			variant_id: "custom".into(),
			adapter: "portable-v1".into(),
			scope: "user".into(),
			format: "zip".into(),
			entry_point: entry_point.map(str::to_owned),
			strip_components: 0,
			evidence: EvidenceLevel::LocalObserved,
			expected_sha256,
		},
		created_at: NOW,
		expires_at: NOW + 900,
	};

	Plan::seal(body)
		.unwrap()
		.approve(
			&Consent {
				source_shown: "o/r".into(),
				acknowledged_unreviewed: true,
			},
			NOW + 1,
		)
		.unwrap()
}

fn meta() -> InstallMeta {
	InstallMeta {
		display_name: "App".into(),
		catalog_slug: None,
	}
}

fn apps_is_empty(fixture: &Fixture) -> bool {
	!fixture.data.apps_dir().exists()
		|| std::fs::read_dir(fixture.data.apps_dir())
			.unwrap()
			.next()
			.is_none()
}

fn staging_is_empty(fixture: &Fixture) -> bool {
	!fixture.data.staging_dir().exists()
		|| std::fs::read_dir(fixture.data.staging_dir())
			.unwrap()
			.next()
			.is_none()
}

#[tokio::test]
async fn installs_records_a_receipt_and_cleans_up() {
	let fixture = fixture().await;
	let mut events = fixture.events.subscribe();

	let id = fixture
		.service
		.install(&plan(&fixture, 1, None, Some("run.sh")), &meta(), NOW)
		.await
		.unwrap();

	let record = fixture.database.installation(id).await.unwrap().unwrap();
	let root = PathBuf::from(&record.root);

	assert_eq!(
		std::fs::read_to_string(root.join("data/info.txt")).unwrap(),
		"hello"
	);
	assert!(root.join(".remade-install").exists());
	assert_eq!(record.entry_point.as_deref(), Some("run.sh"));
	assert_eq!(record.artifact_sha256, digest_of(&fixture.archive));
	assert_eq!(record.evidence, "local-observed");
	assert!(staging_is_empty(&fixture));
	assert!(
		fixture
			.database
			.unfinished_operations()
			.await
			.unwrap()
			.is_empty()
	);

	let mut stages = Vec::new();
	while let Ok(EngineEvent::Operation(event)) = events.try_recv() {
		stages.push(event.stage);
	}

	assert_eq!(stages.first(), Some(&Stage::Accepted));
	assert_eq!(stages.last(), Some(&Stage::Completed));
	assert!(stages.contains(&Stage::Verifying) && stages.contains(&Stage::Extracting));
}

#[cfg(unix)]
#[tokio::test]
async fn launches_the_entry_point_inside_the_app_folder() {
	let fixture = fixture().await;
	let id = fixture
		.service
		.install(&plan(&fixture, 1, None, Some("run.sh")), &meta(), NOW)
		.await
		.unwrap();
	let root = PathBuf::from(
		fixture
			.database
			.installation(id)
			.await
			.unwrap()
			.unwrap()
			.root,
	);

	fixture.service.launch(id).await.unwrap();

	for _ in 0..50 {
		if root.join("ran.txt").exists() {
			return;
		}

		tokio::time::sleep(std::time::Duration::from_millis(50)).await;
	}

	panic!("the app never ran");
}

/// Never finishes a download, so a stop has to interrupt it.
struct HangingArtifacts;

impl ArtifactSource for HangingArtifacts {
	fn fetch<'a>(
		&'a self,
		_url: Url,
		_file: &'a mut tokio::fs::File,
		_max_bytes: u64,
		_progress: &'a mut (dyn FnMut(u64, Option<u64>) + Send),
	) -> Pin<Box<dyn Future<Output = Result<Downloaded, LauncherError>> + Send + 'a>> {
		Box::pin(std::future::pending())
	}
}

/// Finishes the download, then asks the service to stop.
struct StopsAfterDownload {
	inner: LocalArtifacts,
	signal: Arc<std::sync::OnceLock<super::cancel::CancelSignal>>,
}

impl ArtifactSource for StopsAfterDownload {
	fn fetch<'a>(
		&'a self,
		url: Url,
		file: &'a mut tokio::fs::File,
		max_bytes: u64,
		progress: &'a mut (dyn FnMut(u64, Option<u64>) + Send),
	) -> Pin<Box<dyn Future<Output = Result<Downloaded, LauncherError>> + Send + 'a>> {
		Box::pin(async move {
			let downloaded = self.inner.fetch(url, file, max_bytes, progress).await?;

			assert!(self.signal.get().unwrap().request());

			Ok(downloaded)
		})
	}
}

fn with_source(fixture: &Fixture, artifacts: Arc<dyn ArtifactSource>) -> LifecycleService {
	LifecycleService::new(
		fixture.data.clone(),
		Arc::clone(&fixture.database),
		artifacts,
		fixture.events.clone(),
	)
}

fn nothing_left_behind(fixture: &Fixture) {
	assert!(apps_is_empty(fixture));
	assert!(staging_is_empty(fixture));
}

#[tokio::test]
async fn a_stop_during_the_download_installs_nothing() {
	let fixture = fixture().await;
	let service = Arc::new(with_source(&fixture, Arc::new(HangingArtifacts)));
	let mut events = fixture.events.subscribe();
	let approved = plan(&fixture, 1, None, Some("run.sh"));

	let running = {
		let service = Arc::clone(&service);

		tokio::spawn(async move { service.install(&approved, &meta(), NOW).await })
	};

	// Wait until the download has really started.
	loop {
		if let Ok(EngineEvent::Operation(event)) = events.recv().await
			&& matches!(event.stage, Stage::Downloading { .. })
		{
			break;
		}
	}

	assert!(service.request_cancel());

	let error = running.await.unwrap().unwrap_err();

	assert_eq!(error.code(), ErrorCode::Cancelled);
	nothing_left_behind(&fixture);
	assert!(fixture.database.installations().await.unwrap().is_empty());
	assert!(
		fixture
			.database
			.unfinished_operations()
			.await
			.unwrap()
			.is_empty()
	);
	assert!(!service.request_cancel(), "nothing is running any more");
}

#[tokio::test]
async fn a_stop_between_steps_is_honoured_before_files_move() {
	let fixture = fixture().await;
	let signal = Arc::new(std::sync::OnceLock::new());
	let service = with_source(
		&fixture,
		Arc::new(StopsAfterDownload {
			inner: LocalArtifacts(HashMap::from([(URL.to_owned(), fixture.archive.clone())])),
			signal: Arc::clone(&signal),
		}),
	);
	signal.set(service.cancel.clone()).ok();

	let error = service
		.install(&plan(&fixture, 1, None, Some("run.sh")), &meta(), NOW)
		.await
		.unwrap_err();

	assert_eq!(error.code(), ErrorCode::Cancelled);
	nothing_left_behind(&fixture);
}

#[tokio::test]
async fn a_wrong_checksum_installs_nothing() {
	let fixture = fixture().await;
	let wrong = Some("00".repeat(32));

	let error = fixture
		.service
		.install(&plan(&fixture, 1, wrong, None), &meta(), NOW)
		.await
		.unwrap_err();

	assert_eq!(error.code(), ErrorCode::ArtifactDigestMismatch);
	assert!(apps_is_empty(&fixture));
	assert!(staging_is_empty(&fixture));
	assert!(fixture.database.installations().await.unwrap().is_empty());
	assert!(
		fixture
			.database
			.unfinished_operations()
			.await
			.unwrap()
			.is_empty()
	);
}

#[tokio::test]
async fn a_matching_pinned_checksum_is_accepted() {
	let fixture = fixture().await;
	let right = Some(digest_of(&fixture.archive));

	assert!(
		fixture
			.service
			.install(&plan(&fixture, 1, right, None), &meta(), NOW)
			.await
			.is_ok()
	);
}

#[tokio::test]
async fn an_unsafe_archive_is_refused_before_anything_is_written() {
	let fixture = fixture_with(&[("../escape.txt", b"x", 0o644), ("ok.txt", b"y", 0o644)]).await;

	let error = fixture
		.service
		.install(&plan(&fixture, 1, None, None), &meta(), NOW)
		.await
		.unwrap_err();

	assert_eq!(error.code(), ErrorCode::PackageUnsafe);
	assert!(
		!error.user_message().contains("escape"),
		"archive text must not reach the user"
	);
	assert!(apps_is_empty(&fixture));
	assert!(staging_is_empty(&fixture));
}

#[tokio::test]
async fn a_missing_entry_point_is_refused() {
	let fixture = fixture().await;

	let error = fixture
		.service
		.install(&plan(&fixture, 1, None, Some("nope.sh")), &meta(), NOW)
		.await
		.unwrap_err();

	assert_eq!(error.code(), ErrorCode::PackageUnsafe);
	assert!(apps_is_empty(&fixture));
}

#[tokio::test]
async fn unsupported_adapters_and_scopes_are_refused() {
	let fixture = fixture().await;
	let size = std::fs::metadata(&fixture.archive).unwrap().len();

	for (adapter, scope) in [("msi-v1", "user"), ("portable-v1", "system")] {
		let body = PlanBody {
			schema_version: 1,
			operation: Operation::Install,
			origin: Origin::Custom,
			source: SourceBinding {
				repository_id: 1,
				owner_id: 2,
				display: "o/r".into(),
			},
			release: ReleaseBinding {
				id: 1,
				tag: "v1".into(),
			},
			asset: AssetBinding {
				id: 50,
				name: "app.zip".into(),
				size,
				download_url: URL.into(),
			},
			install: InstallBinding {
				variant_id: "v".into(),
				adapter: adapter.into(),
				scope: scope.into(),
				format: "zip".into(),
				entry_point: None,
				strip_components: 0,
				evidence: EvidenceLevel::LocalObserved,
				expected_sha256: None,
			},
			created_at: NOW,
			expires_at: NOW + 900,
		};
		let approved = Plan::seal(body)
			.unwrap()
			.approve(
				&Consent {
					source_shown: "o/r".into(),
					acknowledged_unreviewed: true,
				},
				NOW + 1,
			)
			.unwrap();

		let error = fixture
			.service
			.install(&approved, &meta(), NOW)
			.await
			.unwrap_err();

		assert_eq!(
			error.code(),
			ErrorCode::PackageUnsupported,
			"{adapter}/{scope}"
		);
	}
}

#[tokio::test]
async fn downloads_only_come_from_github_over_https() {
	let fixture = fixture().await;
	let size = std::fs::metadata(&fixture.archive).unwrap().len();

	for url in [
		"http://github.com/o/r/a.zip",
		"https://evil.example/a.zip",
		"https://github.com.evil.example/a.zip",
	] {
		let body = PlanBody {
			schema_version: 1,
			operation: Operation::Install,
			origin: Origin::Custom,
			source: SourceBinding {
				repository_id: 1,
				owner_id: 2,
				display: "o/r".into(),
			},
			release: ReleaseBinding {
				id: 1,
				tag: "v1".into(),
			},
			asset: AssetBinding {
				id: 50,
				name: "a.zip".into(),
				size,
				download_url: url.into(),
			},
			install: InstallBinding {
				variant_id: "v".into(),
				adapter: "portable-v1".into(),
				scope: "user".into(),
				format: "zip".into(),
				entry_point: None,
				strip_components: 0,
				evidence: EvidenceLevel::LocalObserved,
				expected_sha256: None,
			},
			created_at: NOW,
			expires_at: NOW + 900,
		};
		let approved = Plan::seal(body)
			.unwrap()
			.approve(
				&Consent {
					source_shown: "o/r".into(),
					acknowledged_unreviewed: true,
				},
				NOW + 1,
			)
			.unwrap();

		let error = fixture
			.service
			.install(&approved, &meta(), NOW)
			.await
			.unwrap_err();

		assert_eq!(error.code(), ErrorCode::PackageUnsafe, "{url}");
	}
}

#[tokio::test]
async fn uninstall_removes_owned_files_and_keeps_user_data() {
	let fixture = fixture().await;
	let id = fixture
		.service
		.install(&plan(&fixture, 1, None, Some("run.sh")), &meta(), NOW)
		.await
		.unwrap();
	let root = PathBuf::from(
		fixture
			.database
			.installation(id)
			.await
			.unwrap()
			.unwrap()
			.root,
	);

	// The app saved its own data, and the user edited one shipped file.
	std::fs::write(root.join("saved-game.dat"), "progress").unwrap();
	std::fs::write(root.join("data/info.txt"), "edited").unwrap();

	let report = fixture.service.uninstall(id, NOW + 5).await.unwrap();

	assert_eq!(report.kept_unknown, vec!["saved-game.dat".to_owned()]);
	assert_eq!(report.kept_modified, vec!["data/info.txt".to_owned()]);
	assert_eq!(
		report.leftover_folder.as_deref(),
		Some(root.to_str().unwrap())
	);
	assert_eq!(
		std::fs::read_to_string(root.join("saved-game.dat")).unwrap(),
		"progress"
	);
	assert_eq!(
		std::fs::read_to_string(root.join("data/info.txt")).unwrap(),
		"edited"
	);
	assert!(!root.join("run.sh").exists());
	assert!(fixture.database.installation(id).await.unwrap().is_none());
}

#[tokio::test]
async fn a_clean_uninstall_leaves_nothing_behind() {
	let fixture = fixture().await;
	let id = fixture
		.service
		.install(&plan(&fixture, 1, None, None), &meta(), NOW)
		.await
		.unwrap();

	let report = fixture.service.uninstall(id, NOW + 5).await.unwrap();

	assert!(report.is_clean());
	assert!(report.removed_files >= 3);
	assert!(apps_is_empty(&fixture));
	assert!(
		fixture
			.database
			.unfinished_operations()
			.await
			.unwrap()
			.is_empty()
	);
}

#[tokio::test]
async fn uninstall_refuses_a_folder_that_is_not_ours() {
	let fixture = fixture().await;
	let id = fixture
		.service
		.install(&plan(&fixture, 1, None, None), &meta(), NOW)
		.await
		.unwrap();
	let root = PathBuf::from(
		fixture
			.database
			.installation(id)
			.await
			.unwrap()
			.unwrap()
			.root,
	);

	std::fs::remove_file(root.join(".remade-install")).unwrap();

	let error = fixture.service.uninstall(id, NOW + 5).await.unwrap_err();

	assert_eq!(error.code(), ErrorCode::InstallationChanged);
	assert_eq!(
		std::fs::read_to_string(root.join("data/info.txt")).unwrap(),
		"hello"
	);
	assert!(fixture.database.installation(id).await.unwrap().is_some());
}

#[tokio::test]
async fn a_tampered_manifest_path_cannot_delete_outside_the_app() {
	let fixture = fixture().await;
	let id = fixture
		.service
		.install(&plan(&fixture, 1, None, None), &meta(), NOW)
		.await
		.unwrap();
	let outside = fixture.data.root().join("precious.txt");
	std::fs::write(&outside, "keep").unwrap();

	// Rewrite the receipt as if the database had been edited.
	let record = fixture.database.installation(id).await.unwrap().unwrap();
	let evil = serde_json::json!([{"path": "../../../precious.txt", "kind": "file", "size": 4, "sha256": ""}]);
	let mut tampered = record.clone();
	tampered.manifest = evil.to_string();
	let op = OperationId::new();
	fixture
		.database
		.begin_operation(OperationRecord {
			id: op,
			kind: "update".into(),
			installation_id: Some(id),
			plan_digest: [0; 32],
			plan: "{}".into(),
			state: "accepted".into(),
			staging: None,
			detail: None,
			created_at: NOW,
			updated_at: NOW,
		})
		.await
		.unwrap();
	fixture
		.database
		.commit_installation(op, tampered, "test".into(), NOW)
		.await
		.unwrap();

	let error = fixture.service.uninstall(id, NOW + 5).await.unwrap_err();

	assert_eq!(error.code(), ErrorCode::InstallationChanged);
	assert_eq!(std::fs::read_to_string(&outside).unwrap(), "keep");
}

#[tokio::test]
async fn an_update_installs_the_new_version_then_removes_the_old_one() {
	let fixture = fixture().await;
	let id = fixture
		.service
		.install(&plan(&fixture, 1, None, None), &meta(), NOW)
		.await
		.unwrap();
	let old_root = PathBuf::from(
		fixture
			.database
			.installation(id)
			.await
			.unwrap()
			.unwrap()
			.root,
	);

	// A new release of the same app, with new contents.
	write_zip(
		&fixture.archive,
		&[
			("run.sh", SCRIPT, 0o755),
			("data/info.txt", b"version two", 0o644),
		],
	);

	fixture
		.service
		.update(id, &plan(&fixture, 2, None, None), &meta(), NOW + 10)
		.await
		.unwrap();

	let record = fixture.database.installation(id).await.unwrap().unwrap();
	let new_root = PathBuf::from(&record.root);

	assert_ne!(old_root, new_root);
	assert_eq!(record.release_tag, "v2");
	assert_eq!(
		record.installed_at, NOW,
		"the install date survives an update"
	);
	assert_eq!(
		std::fs::read_to_string(new_root.join("data/info.txt")).unwrap(),
		"version two"
	);
	assert!(!old_root.exists());
	assert_eq!(fixture.database.installations().await.unwrap().len(), 1);
}

#[tokio::test]
async fn updating_something_that_is_not_installed_fails_cleanly() {
	let fixture = fixture().await;

	let error = fixture
		.service
		.update(
			InstallationId::new(),
			&plan(&fixture, 2, None, None),
			&meta(),
			NOW,
		)
		.await
		.unwrap_err();

	assert_eq!(error.code(), ErrorCode::InstallationNotFound);
}

async fn journal(
	fixture: &Fixture,
	kind: &str,
	state: &str,
	installation: Option<InstallationId>,
	detail: Option<String>,
) -> OperationId {
	let id = OperationId::new();

	fixture
		.database
		.begin_operation(OperationRecord {
			id,
			kind: kind.into(),
			installation_id: installation,
			plan_digest: [1; 32],
			plan: "{}".into(),
			state: state.into(),
			staging: None,
			detail,
			created_at: NOW,
			updated_at: NOW,
		})
		.await
		.unwrap();

	id
}

#[tokio::test]
async fn recovery_discards_staging_for_an_install_interrupted_before_apply() {
	let fixture = fixture().await;

	for stage in [state::ACCEPTED, state::DOWNLOADING, state::EXTRACTING] {
		let id = journal(
			&fixture,
			"install",
			stage,
			Some(InstallationId::new()),
			None,
		)
		.await;
		let staging = fixture.data.staging_dir().join(id.to_string());
		std::fs::create_dir_all(staging.join("tree")).unwrap();
		std::fs::write(staging.join("artifact"), "partial").unwrap();
	}

	let report = fixture.service.recover(NOW + 100).await.unwrap();

	assert_eq!(report.rolled_back, 3);
	assert!(staging_is_empty(&fixture));
	assert!(
		fixture
			.database
			.unfinished_operations()
			.await
			.unwrap()
			.is_empty()
	);
	assert!(apps_is_empty(&fixture));
}

#[tokio::test]
async fn recovery_rolls_back_files_moved_into_place_without_a_receipt() {
	let fixture = fixture().await;
	let installation = InstallationId::new();
	let op = OperationId::new();
	let root = fixture
		.data
		.apps_dir()
		.join(installation.to_string())
		.join("1-50");
	std::fs::create_dir_all(&root).unwrap();
	std::fs::write(root.join("file"), "x").unwrap();
	std::fs::write(
		root.join(".remade-install"),
		serde_json::json!({"installation": installation.to_string(), "operation": op.to_string()})
			.to_string(),
	)
	.unwrap();

	fixture
		.database
		.begin_operation(OperationRecord {
			id: op,
			kind: "install".into(),
			installation_id: Some(installation),
			plan_digest: [1; 32],
			plan: "{}".into(),
			state: state::APPLYING.into(),
			staging: None,
			detail: Some(root.display().to_string()),
			created_at: NOW,
			updated_at: NOW,
		})
		.await
		.unwrap();

	let report = fixture.service.recover(NOW + 100).await.unwrap();

	assert_eq!(report.rolled_back, 1);
	assert!(apps_is_empty(&fixture));
}

#[tokio::test]
async fn recovery_never_deletes_a_folder_whose_marker_names_another_operation() {
	let fixture = fixture().await;
	let installation = InstallationId::new();
	let root = fixture
		.data
		.apps_dir()
		.join(installation.to_string())
		.join("1-50");
	std::fs::create_dir_all(&root).unwrap();
	std::fs::write(root.join("file"), "mine").unwrap();
	std::fs::write(
		root.join(".remade-install"),
		serde_json::json!({"installation": installation.to_string(), "operation": OperationId::new().to_string()}).to_string(),
	)
	.unwrap();

	journal(
		&fixture,
		"install",
		state::APPLYING,
		Some(installation),
		Some(root.display().to_string()),
	)
	.await;

	fixture.service.recover(NOW + 100).await.unwrap();

	assert_eq!(std::fs::read_to_string(root.join("file")).unwrap(), "mine");
}

#[tokio::test]
async fn recovery_finishes_a_committed_install_whose_journal_write_was_lost() {
	let fixture = fixture().await;
	let id = fixture
		.service
		.install(&plan(&fixture, 1, None, None), &meta(), NOW)
		.await
		.unwrap();
	let record = fixture.database.installation(id).await.unwrap().unwrap();

	journal(
		&fixture,
		"install",
		state::APPLYING,
		Some(id),
		Some(record.root.clone()),
	)
	.await;

	let report = fixture.service.recover(NOW + 100).await.unwrap();

	assert_eq!(report.completed, 1);
	assert!(
		PathBuf::from(&record.root).exists(),
		"a committed install is kept"
	);
}

#[tokio::test]
async fn recovery_finishes_an_interrupted_uninstall() {
	let fixture = fixture().await;
	let id = fixture
		.service
		.install(&plan(&fixture, 1, None, None), &meta(), NOW)
		.await
		.unwrap();
	let root = PathBuf::from(
		fixture
			.database
			.installation(id)
			.await
			.unwrap()
			.unwrap()
			.root,
	);

	// The crash happened after some files were removed.
	std::fs::remove_file(root.join("run.sh")).unwrap();
	journal(&fixture, "uninstall", state::REMOVING, Some(id), None).await;

	let report = fixture.service.recover(NOW + 100).await.unwrap();

	assert_eq!(report.completed, 1);
	assert!(fixture.database.installation(id).await.unwrap().is_none());
	assert!(apps_is_empty(&fixture));
}

#[tokio::test]
async fn recovery_is_idempotent() {
	let fixture = fixture().await;
	journal(
		&fixture,
		"install",
		state::EXTRACTING,
		Some(InstallationId::new()),
		None,
	)
	.await;

	let first = fixture.service.recover(NOW + 1).await.unwrap();
	let second = fixture.service.recover(NOW + 2).await.unwrap();

	assert_eq!(first.rolled_back, 1);
	assert_eq!(second, RecoveryReport::default());
}

#[tokio::test]
async fn a_running_operation_in_another_process_blocks_recovery_and_new_installs() {
	let fixture = fixture().await;
	journal(
		&fixture,
		"install",
		state::EXTRACTING,
		Some(InstallationId::new()),
		None,
	)
	.await;

	// Stands in for the other process, which is mid-operation.
	let held = super::lock::acquire(&fixture.data).unwrap();

	let report = fixture.service.recover(NOW + 1).await.unwrap();
	assert_eq!(
		report,
		RecoveryReport::default(),
		"a live operation is not a crashed one"
	);
	assert_eq!(
		fixture
			.database
			.unfinished_operations()
			.await
			.unwrap()
			.len(),
		1
	);

	let busy = fixture
		.service
		.install(&plan(&fixture, 1, None, None), &meta(), NOW)
		.await
		.unwrap_err();
	assert_eq!(busy.code(), ErrorCode::OperationInProgress);

	drop(held);

	assert_eq!(
		fixture.service.recover(NOW + 2).await.unwrap().rolled_back,
		1
	);
	assert!(
		fixture
			.service
			.install(&plan(&fixture, 1, None, None), &meta(), NOW)
			.await
			.is_ok()
	);
}

#[tokio::test]
async fn a_single_executable_one_folder_down_becomes_the_entry_point() {
	let fixture = fixture_with(&[
		("tool-1.0/tool", SCRIPT, 0o755),
		("tool-1.0/README", b"x", 0o644),
	])
	.await;

	let id = fixture
		.service
		.install(&plan(&fixture, 1, None, None), &meta(), NOW)
		.await
		.unwrap();

	let record = fixture.database.installation(id).await.unwrap().unwrap();
	assert_eq!(record.entry_point.as_deref(), Some("tool-1.0/tool"));
}

#[tokio::test]
async fn several_executables_leave_the_entry_point_unset_instead_of_guessing() {
	let fixture = fixture_with(&[("a", SCRIPT, 0o755), ("b", SCRIPT, 0o755)]).await;

	let id = fixture
		.service
		.install(&plan(&fixture, 1, None, None), &meta(), NOW)
		.await
		.unwrap();

	assert_eq!(
		fixture
			.database
			.installation(id)
			.await
			.unwrap()
			.unwrap()
			.entry_point,
		None
	);
}
