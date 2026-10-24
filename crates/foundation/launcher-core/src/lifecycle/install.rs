use std::path::PathBuf;
use std::sync::Arc;

use launcher_model::{ErrorCode, InstallationId, LauncherError, OperationId};
use launcher_net::Url;
use launcher_packages::{Format, Inventory, Limits, OwnedFile, OwnedKind, PackageError};
use launcher_resolver::{ApprovedPlan, Origin, PlanBody};
use launcher_storage::{Database, InstallationRecord, OperationRecord};
use sha2::{Digest, Sha256};

use super::artifact::ArtifactSource;
use super::cancel::CancelSignal;
use super::meta::{InstallMeta, ManifestEntry};
use super::safe_delete::{MARKER, remove_staging};
use crate::events::{EngineEvent, EventBus, OperationEvent, Stage};
use crate::paths::DataDirectory;

pub(crate) mod state {
	pub const ACCEPTED: &str = "accepted";
	pub const DOWNLOADING: &str = "downloading";
	pub const VERIFYING: &str = "verifying";
	pub const INSPECTING: &str = "inspecting";
	pub const EXTRACTING: &str = "extracting";
	pub const APPLYING: &str = "applying";
	pub const REMOVING: &str = "removing";
	pub const FAILED: &str = "failed";
	pub const RECOVERY_REQUIRED: &str = "recovery_required";
}

/// Hard ceiling for one artifact, whatever the provider claims.
const MAX_ARTIFACT_BYTES: u64 = 2 * 1024 * 1024 * 1024;

/// The adapter this build implements.
pub(crate) const PORTABLE_ADAPTER: &str = "portable-v1";

pub struct LifecycleService {
	pub(crate) data: DataDirectory,
	pub(crate) database: Arc<Database>,
	pub(crate) artifacts: Arc<dyn ArtifactSource>,
	pub(crate) events: EventBus,
	pub(crate) cancel: CancelSignal,
}

impl LifecycleService {
	pub(crate) fn new(
		data: DataDirectory,
		database: Arc<Database>,
		artifacts: Arc<dyn ArtifactSource>,
		events: EventBus,
	) -> Self {
		Self {
			data,
			database,
			artifacts,
			events,
			cancel: CancelSignal::default(),
		}
	}

	/// Asks the running install or update to stop. It succeeds only before
	/// files start moving into place; after that the operation finishes.
	pub fn request_cancel(&self) -> bool {
		self.cancel.request()
	}

	/// Installs the approved plan as a new app.
	pub async fn install(
		&self,
		approved: &ApprovedPlan,
		meta: &InstallMeta,
		now: i64,
	) -> Result<InstallationId, LauncherError> {
		self.apply(approved, meta, InstallationId::new(), None, now)
			.await
	}

	/// Replaces an installed app's files with the approved plan's release.
	/// The old version is removed only after the new one is committed.
	pub async fn update(
		&self,
		installation: InstallationId,
		approved: &ApprovedPlan,
		meta: &InstallMeta,
		now: i64,
	) -> Result<InstallationId, LauncherError> {
		let previous = self
			.database
			.installation(installation)
			.await?
			.ok_or_else(not_found)?;

		self.apply(approved, meta, installation, Some(previous), now)
			.await
	}

	async fn apply(
		&self,
		approved: &ApprovedPlan,
		meta: &InstallMeta,
		installation: InstallationId,
		previous: Option<InstallationRecord>,
		now: i64,
	) -> Result<InstallationId, LauncherError> {
		// One operation at a time, across the app and the CLI.
		let _lock = super::lock::acquire_patiently(&self.data).await?;

		self.cancel.begin();

		let result = self
			.apply_locked(approved, meta, installation, previous, now)
			.await;

		self.cancel.finish();

		result
	}

	async fn apply_locked(
		&self,
		approved: &ApprovedPlan,
		meta: &InstallMeta,
		installation: InstallationId,
		previous: Option<InstallationRecord>,
		now: i64,
	) -> Result<InstallationId, LauncherError> {
		let plan = approved.plan();
		let body = plan.body();
		let operation = OperationId::new();

		let record = OperationRecord {
			id: operation,
			kind: if previous.is_some() {
				"update"
			} else {
				"install"
			}
			.into(),
			installation_id: Some(installation),
			plan_digest: plan.digest(),
			plan: serde_json::to_string(body).map_err(internal)?,
			state: state::ACCEPTED.into(),
			staging: None,
			detail: None,
			created_at: now,
			updated_at: now,
		};

		self.database.begin_operation(record).await?;
		self.publish(operation, installation, Stage::Accepted);

		let run = Run {
			service: self,
			operation,
			installation,
			body,
			meta,
			previous: previous.as_ref(),
			now,
		};

		match run.execute().await {
			Ok(()) => {
				self.publish(operation, installation, Stage::Completed);

				Ok(installation)
			}
			Err(error) => {
				tracing::warn!(%error, %operation, "installation failed");
				self.fail(operation, installation, &error, now).await;

				Err(error)
			}
		}
	}

	pub(crate) fn publish(
		&self,
		operation: OperationId,
		installation: InstallationId,
		stage: Stage,
	) {
		self.events.publish(EngineEvent::Operation(OperationEvent {
			operation,
			installation,
			stage,
		}));
	}

	/// Cleans staging and records the failure. If the install had already
	/// moved files into place, recovery will handle them instead.
	async fn fail(
		&self,
		operation: OperationId,
		installation: InstallationId,
		error: &LauncherError,
		now: i64,
	) {
		let applying = matches!(
			self.database.operation(operation).await,
			Ok(Some(record)) if record.state == state::APPLYING
		);

		if applying {
			let message =
				"The install stopped while moving files into place. Recovery will tidy up."
					.to_owned();

			let _ = self
				.database
				.set_operation_state(
					operation,
					state::RECOVERY_REQUIRED,
					None,
					Some(error.to_string()),
					now,
				)
				.await;
			self.publish(operation, installation, Stage::RecoveryRequired { message });

			return;
		}

		if let Err(io_error) = remove_staging(&self.data.staging_dir(), &operation.to_string()) {
			tracing::warn!(%io_error, "could not remove staging after a failed install");
		}

		let _ = self
			.database
			.set_operation_state(operation, state::FAILED, None, Some(error.to_string()), now)
			.await;
		self.publish(
			operation,
			installation,
			Stage::Failed {
				message: error.user_message().to_owned(),
			},
		);
	}
}

pub(crate) fn not_found() -> LauncherError {
	LauncherError::new(
		ErrorCode::InstallationNotFound,
		"That app is not installed.",
	)
}

pub(crate) fn internal(error: impl std::error::Error + Send + Sync + 'static) -> LauncherError {
	LauncherError::new(
		ErrorCode::InternalInvariant,
		"The launcher hit an unexpected problem.",
	)
	.with_source(error)
}

struct Run<'a> {
	service: &'a LifecycleService,
	operation: OperationId,
	installation: InstallationId,
	body: &'a PlanBody,
	meta: &'a InstallMeta,
	previous: Option<&'a InstallationRecord>,
	now: i64,
}

impl Run<'_> {
	async fn execute(&self) -> Result<(), LauncherError> {
		let service = self.service;
		let staging = service.data.staging_dir().join(self.operation.to_string());

		check_adapter(self.body)?;

		self.step(
			state::DOWNLOADING,
			Stage::Downloading {
				received: 0,
				total: None,
			},
			Some(&staging),
		)
		.await?;
		std::fs::create_dir_all(&staging).map_err(io_error)?;

		let artifact_path = staging.join("artifact");
		let observed = self.download(&artifact_path).await?;

		self.step(state::VERIFYING, Stage::Verifying, None).await?;
		self.verify(&observed)?;

		self.step(state::INSPECTING, Stage::Inspecting, None)
			.await?;
		let (format, inventory) = self.inspect(artifact_path.clone()).await?;
		let entry_point = self.resolve_entry_point(&inventory)?;

		self.step(state::EXTRACTING, Stage::Extracting, None)
			.await?;
		let tree = staging.join("tree");
		let mut owned = self.extract(artifact_path, inventory, tree.clone()).await?;
		owned.push(self.write_marker(&tree)?);

		// Without this, macOS would never check a downloaded app's signature.
		super::quarantine::mark_downloaded(&tree, self.now)?;

		let root = self.final_root();

		// The last safe point. Past it the install runs to the end.
		service.cancel.commit()?;

		self.step(state::APPLYING, Stage::Applying, None).await?;
		service
			.database
			.set_operation_state(
				self.operation,
				state::APPLYING,
				None,
				Some(root.display().to_string()),
				self.now,
			)
			.await?;
		move_into_place(&tree, &root)?;

		let record = self.receipt(&observed.sha256, format, entry_point, &root, &owned)?;

		service
			.database
			.commit_installation(
				self.operation,
				record,
				format!(
					"{} {}",
					if self.previous.is_some() {
						"Updated to"
					} else {
						"Installed"
					},
					self.body.release.tag
				),
				self.now,
			)
			.await?;

		if let Some(previous) = self.previous {
			// The new version is live. A leftover old version is reported,
			// not fatal.
			if let Err(error) = service.remove_installed_files(previous).await {
				tracing::warn!(%error, "the previous version could not be fully removed");
			}
		}

		if let Err(error) = remove_staging(&service.data.staging_dir(), &self.operation.to_string())
		{
			tracing::warn!(%error, "could not remove staging after an install");
		}

		Ok(())
	}

	async fn step(
		&self,
		state: &'static str,
		stage: Stage,
		staging: Option<&PathBuf>,
	) -> Result<(), LauncherError> {
		// Every step before the files move is a safe place to stop.
		self.service.cancel.check()?;

		self.service
			.database
			.set_operation_state(
				self.operation,
				state,
				staging.map(|path| path.display().to_string()),
				None,
				self.now,
			)
			.await?;
		self.service
			.publish(self.operation, self.installation, stage);

		Ok(())
	}

	async fn download(&self, path: &std::path::Path) -> Result<Observed, LauncherError> {
		let asset = &self.body.asset;
		let url = Url::parse(&asset.download_url).map_err(|_| unsafe_source())?;

		if url.scheme() != "https" || url.host_str() != Some("github.com") {
			return Err(unsafe_source());
		}

		let mut file = tokio::fs::OpenOptions::new()
			.write(true)
			.create_new(true)
			.open(path)
			.await
			.map_err(io_error)?;

		let limit = if asset.size > 0 {
			asset.size.min(MAX_ARTIFACT_BYTES)
		} else {
			MAX_ARTIFACT_BYTES
		};
		let (operation, installation) = (self.operation, self.installation);
		let mut last_percent = u64::MAX;

		let mut progress = |received: u64, total: Option<u64>| {
			// Publish roughly once per percent so a fast link cannot flood
			// the interface.
			let percent = total.map_or(received / (4 * 1024 * 1024), |total| {
				received * 100 / total.max(1)
			});

			if percent != last_percent {
				last_percent = percent;
				self.service.publish(
					operation,
					installation,
					Stage::Downloading { received, total },
				);
			}
		};

		// Dropping the transfer when a stop is requested closes the
		// connection; staging is removed by the failure path.
		let downloaded = tokio::select! {
			biased;
			() = self.service.cancel.requested() => {
				return Err(LauncherError::new(
					ErrorCode::Cancelled,
					"Stopped. Nothing was installed.",
				));
			}
			result = self.service.artifacts.fetch(url, &mut file, limit, &mut progress) => result?,
		};

		Ok(Observed {
			size: downloaded.size,
			sha256: downloaded.sha256,
		})
	}

	fn verify(&self, observed: &Observed) -> Result<(), LauncherError> {
		let mismatch =
			|why: &str| LauncherError::new(ErrorCode::ArtifactDigestMismatch, why.to_owned());

		if self.body.asset.size > 0 && observed.size != self.body.asset.size {
			return Err(mismatch(
				"The downloaded file is not the size GitHub reported.",
			));
		}

		match &self.body.install.expected_sha256 {
			Some(expected) if *expected != observed.sha256 => Err(mismatch(
				"The downloaded file does not match its expected checksum.",
			)),
			_ => Ok(()),
		}
	}

	async fn inspect(&self, artifact: PathBuf) -> Result<(Format, Inventory), LauncherError> {
		let format = match self.body.install.format.as_str() {
			"zip" => Format::Zip,
			"tar.gz" => Format::TarGz,
			_ => {
				return Err(unsupported(
					"This package type cannot be installed by the launcher yet.",
				));
			}
		};
		let strip = self.body.install.strip_components;

		let inventory = tokio::task::spawn_blocking(move || {
			launcher_packages::inspect(&artifact, format, strip, Limits::default())
		})
		.await
		.map_err(internal)?
		.map_err(package_error)?;

		Ok((format, inventory))
	}

	fn resolve_entry_point(&self, inventory: &Inventory) -> Result<Option<String>, LauncherError> {
		match &self.body.install.entry_point {
			Some(entry) => {
				if !launcher_packages::is_safe_relative_path(entry) || !inventory.has_path(entry) {
					return Err(package_error(PackageError::EntryPointMissing(
						entry.clone(),
					)));
				}

				Ok(Some(entry.clone()))
			}
			None => Ok(detect_entry_point(inventory)),
		}
	}

	async fn extract(
		&self,
		artifact: PathBuf,
		inventory: Inventory,
		tree: PathBuf,
	) -> Result<Vec<OwnedFile>, LauncherError> {
		tokio::task::spawn_blocking(move || {
			launcher_packages::extract(&artifact, &inventory, &tree)
		})
		.await
		.map_err(internal)?
		.map_err(package_error)
	}

	/// Drops a file at the root of the tree naming its owner, so uninstall
	/// can prove the folder is ours before it removes anything.
	fn write_marker(&self, tree: &std::path::Path) -> Result<OwnedFile, LauncherError> {
		let contents = serde_json::json!({
			"installation": self.installation.to_string(),
			"operation": self.operation.to_string(),
			"plan": hex::encode(self.service_plan_digest()),
		})
		.to_string();

		std::fs::write(tree.join(MARKER), &contents).map_err(io_error)?;

		Ok(OwnedFile {
			path: MARKER.into(),
			kind: OwnedKind::File,
			size: contents.len() as u64,
			sha256: hex::encode(Sha256::digest(contents.as_bytes())),
		})
	}

	fn service_plan_digest(&self) -> [u8; 32] {
		let canonical = serde_json::to_vec(self.body).unwrap_or_default();

		Sha256::digest(canonical).into()
	}

	/// Where this release's files go. Putting the same release back over a
	/// damaged copy, as a repair does, needs a folder of its own beside the
	/// old one, which is removed once the new one is committed.
	fn final_root(&self) -> PathBuf {
		let base = self
			.service
			.data
			.apps_dir()
			.join(self.installation.to_string());
		let name = format!("{}-{}", self.body.release.id, self.body.asset.id);
		let first = base.join(&name);

		if !first.exists() {
			return first;
		}

		(2..100)
			.map(|attempt| base.join(format!("{name}-r{attempt}")))
			.find(|candidate| !candidate.exists())
			.unwrap_or(first)
	}

	fn receipt(
		&self,
		sha256: &str,
		format: Format,
		entry_point: Option<String>,
		root: &std::path::Path,
		owned: &[OwnedFile],
	) -> Result<InstallationRecord, LauncherError> {
		let manifest: Vec<ManifestEntry> = owned
			.iter()
			.map(|file| ManifestEntry {
				path: file.path.clone(),
				kind: match file.kind {
					OwnedKind::File => "file",
					OwnedKind::Directory => "dir",
					OwnedKind::Symlink => "link",
				}
				.into(),
				size: file.size,
				sha256: file.sha256.clone(),
			})
			.collect();

		let (origin, slug) = match &self.body.origin {
			Origin::Curated { slug, .. } => ("curated", Some(slug.clone())),
			Origin::Custom => ("custom", None),
		};

		Ok(InstallationRecord {
			id: self.installation,
			origin: origin.into(),
			catalog_slug: slug.or_else(|| self.meta.catalog_slug.clone()),
			display_name: self.meta.display_name.clone(),
			repository: self.body.source.display.clone(),
			repository_id: self.body.source.repository_id,
			owner_id: self.body.source.owner_id,
			release_id: self.body.release.id,
			release_tag: self.body.release.tag.clone(),
			asset_id: self.body.asset.id,
			asset_name: self.body.asset.name.clone(),
			artifact_sha256: sha256.to_owned(),
			evidence: serde_json::to_value(self.body.install.evidence)
				.ok()
				.and_then(|value| value.as_str().map(str::to_owned))
				.unwrap_or_default(),
			variant_id: self.body.install.variant_id.clone(),
			adapter: self.body.install.adapter.clone(),
			format: match format {
				Format::Zip => "zip",
				Format::TarGz => "tar.gz",
			}
			.into(),
			root: root.display().to_string(),
			entry_point,
			manifest: serde_json::to_string(&manifest).map_err(internal)?,
			installed_at: self
				.previous
				.map_or(self.now, |previous| previous.installed_at),
			updated_at: self.now,
		})
	}
}

struct Observed {
	size: u64,
	sha256: String,
}

fn check_adapter(body: &PlanBody) -> Result<(), LauncherError> {
	if body.install.adapter != PORTABLE_ADAPTER {
		return Err(unsupported(
			"This app needs an installer type the launcher does not support yet.",
		));
	}

	if body.install.scope != "user" {
		return Err(unsupported(
			"Only per-user installs are supported in this version.",
		));
	}

	Ok(())
}

/// Moves `tree` to `root` without overwriting anything.
fn move_into_place(tree: &std::path::Path, root: &std::path::Path) -> Result<(), LauncherError> {
	if root.exists() {
		return Err(LauncherError::new(
			ErrorCode::InstallationChanged,
			"A folder for this version already exists, so nothing was changed.",
		));
	}

	if let Some(parent) = root.parent() {
		std::fs::create_dir_all(parent).map_err(io_error)?;
	}

	std::fs::rename(tree, root).map_err(io_error)
}

/// A single top-level `.app` bundle, or the single executable at the top
/// level or one folder down. Anything less clear returns `None`, and the app
/// is installed without a launch button instead of guessing.
pub(crate) fn detect_entry_point(inventory: &Inventory) -> Option<String> {
	let mut bundles: Vec<&str> = inventory
		.entries
		.iter()
		.filter_map(|entry| entry.path.split('/').next())
		.filter(|top| top.ends_with(".app"))
		.collect();

	bundles.sort_unstable();
	bundles.dedup();

	if let [only] = bundles.as_slice() {
		return Some((*only).to_owned());
	}

	// Archives usually either hold the program at the top, or wrap everything
	// in one folder. Look at both levels, and only accept a single answer.
	let executables: Vec<&str> = inventory
		.entries
		.iter()
		.filter(|entry| {
			entry.executable
				&& entry.kind == launcher_packages::EntryKind::File
				&& entry.path.matches('/').count() <= 1
		})
		.map(|entry| entry.path.as_str())
		.collect();

	match executables.as_slice() {
		[only] => Some((*only).to_owned()),
		_ => None,
	}
}

pub(crate) fn io_error(error: std::io::Error) -> LauncherError {
	LauncherError::new(
		ErrorCode::StorageUnavailable,
		"The launcher could not write to its folder.",
	)
	.with_source(error)
}

fn unsupported(message: &str) -> LauncherError {
	LauncherError::new(ErrorCode::PackageUnsupported, message.to_owned())
}

fn unsafe_source() -> LauncherError {
	LauncherError::new(
		ErrorCode::PackageUnsafe,
		"The download address is not an https github.com release link.",
	)
}

fn package_error(error: PackageError) -> LauncherError {
	let code = match &error {
		PackageError::Unsupported(_) | PackageError::FormatMismatch => {
			ErrorCode::PackageUnsupported
		}
		PackageError::Unreadable(_) => ErrorCode::StorageUnavailable,
		_ => ErrorCode::PackageUnsafe,
	};

	// The text of `error` can quote archive contents, so it stays in the
	// source chain and the user sees a fixed sentence.
	let message = match code {
		ErrorCode::PackageUnsupported => {
			"The package was not installed. It is not a type the launcher can install."
		}
		ErrorCode::StorageUnavailable => "The package could not be read from disk.",
		_ => "The package was not installed. It contains something the launcher considers unsafe.",
	};

	LauncherError::new(code, message).with_source(error)
}
