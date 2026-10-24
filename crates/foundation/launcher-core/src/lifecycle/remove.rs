//! Removing an installed app, and only what the launcher created.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use launcher_model::{ErrorCode, InstallationId, LauncherError, OperationId};
use launcher_storage::{InstallationRecord, OperationRecord};
use sha2::{Digest, Sha256};

use super::install::{LifecycleService, internal, io_error, not_found, state};
use super::meta::ManifestEntry;
use super::safe_delete::{MARKER, is_direct_child_dir};
use crate::events::Stage;

/// What an uninstall did, in terms a person can act on.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RemovalReport {
	pub removed_files: usize,
	/// Files the app owns that were edited after install. Kept.
	pub kept_modified: Vec<String>,
	/// Files nobody told the launcher about, such as the app's own saved
	/// data. Kept.
	pub kept_unknown: Vec<String>,
	/// Where the leftovers are, if any.
	pub leftover_folder: Option<String>,
}

impl RemovalReport {
	pub fn is_clean(&self) -> bool {
		self.kept_modified.is_empty() && self.kept_unknown.is_empty()
	}
}

impl LifecycleService {
	/// Uninstalls `installation`.
	pub async fn uninstall(
		&self,
		installation: InstallationId,
		now: i64,
	) -> Result<RemovalReport, LauncherError> {
		let record = self
			.database
			.installation(installation)
			.await?
			.ok_or_else(not_found)?;
		let operation = OperationId::new();

		self.database
			.begin_operation(OperationRecord {
				id: operation,
				kind: "uninstall".into(),
				installation_id: Some(installation),
				plan_digest: Sha256::digest(record.id.as_bytes()).into(),
				plan: serde_json::json!({
					"uninstall": record.id.to_string(),
					"root": record.root,
					"source": { "display": record.repository },
				})
				.to_string(),
				state: state::REMOVING.into(),
				staging: None,
				detail: None,
				created_at: now,
				updated_at: now,
			})
			.await?;
		self.publish(operation, installation, Stage::Removing);

		match self.remove_installed_files(&record).await {
			Ok(report) => {
				let detail = if report.is_clean() {
					format!("Removed {} files", report.removed_files)
				} else {
					format!(
						"Removed {} files; kept {} that were edited or are not the launcher's",
						report.removed_files,
						report.kept_modified.len() + report.kept_unknown.len()
					)
				};

				self.database
					.commit_removal(operation, installation, detail, now)
					.await?;
				self.publish(operation, installation, Stage::Completed);

				Ok(report)
			}
			Err(error) => {
				let _ = self
					.database
					.set_operation_state(
						operation,
						state::FAILED,
						None,
						Some(error.to_string()),
						now,
					)
					.await;
				self.publish(
					operation,
					installation,
					Stage::Failed {
						message: error.user_message().to_owned(),
					},
				);

				Err(error)
			}
		}
	}

	/// Removes the files in `record`'s manifest after re-proving ownership.
	pub(crate) async fn remove_installed_files(
		&self,
		record: &InstallationRecord,
	) -> Result<RemovalReport, LauncherError> {
		let apps = self.data.apps_dir();
		let record = record.clone();

		tokio::task::spawn_blocking(move || remove_blocking(&apps, &record))
			.await
			.map_err(internal)?
	}
}

/// The install root must be `apps/<installation-id>/<version>` and carry a
/// marker naming this installation. A path in the database is only a claim.
pub(crate) fn verify_ownership(
	apps: &Path,
	record: &InstallationRecord,
) -> Result<PathBuf, LauncherError> {
	let changed = |why: &str| LauncherError::new(ErrorCode::InstallationChanged, why.to_owned());

	let root = PathBuf::from(&record.root);
	let app_folder = apps.join(record.id.to_string());

	if !is_direct_child_dir(apps, &app_folder) || !is_direct_child_dir(&app_folder, &root) {
		return Err(changed(
			"The app's folder is no longer where the launcher put it, so nothing was removed.",
		));
	}

	let marker = std::fs::read_to_string(root.join(MARKER))
		.ok()
		.and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
		.and_then(|value| value["installation"].as_str().map(str::to_owned));

	if marker.as_deref() != Some(record.id.to_string().as_str()) {
		return Err(changed(
			"The app's folder is not marked as the launcher's, so nothing was removed.",
		));
	}

	Ok(root)
}

fn remove_blocking(
	apps: &Path,
	record: &InstallationRecord,
) -> Result<RemovalReport, LauncherError> {
	let root = match verify_ownership(apps, record) {
		Ok(root) => root,
		// Already gone is fine for removal: there is nothing left to own.
		Err(_) if !Path::new(&record.root).exists() => return Ok(RemovalReport::default()),
		Err(error) => return Err(error),
	};

	let manifest: Vec<ManifestEntry> = serde_json::from_str(&record.manifest).map_err(internal)?;
	let mut report = RemovalReport::default();
	let mut known: HashSet<String> = HashSet::new();
	let mut directories = Vec::new();

	for entry in &manifest {
		if !launcher_packages::is_safe_relative_path(&entry.path) {
			return Err(LauncherError::new(
				ErrorCode::InstallationChanged,
				"The app's record is damaged, so nothing was removed.",
			));
		}

		known.insert(entry.path.clone());
		let path = root.join(&entry.path);

		match entry.kind.as_str() {
			"dir" => directories.push(entry.path.clone()),
			"link" => {
				// Remove the link itself, never what it points at.
				if std::fs::symlink_metadata(&path).is_ok_and(|meta| meta.file_type().is_symlink())
				{
					std::fs::remove_file(&path).map_err(io_error)?;
					report.removed_files += 1;
				}
			}
			_ => remove_file_if_unmodified(&path, entry, &mut report)?,
		}
	}

	// Deepest directories first. A directory that still holds anything is
	// left alone; `remove_dir` refuses to delete a non-empty one.
	directories.sort_by_key(|path| std::cmp::Reverse(path.matches('/').count()));

	for directory in &directories {
		let _ = std::fs::remove_dir(root.join(directory));
	}

	collect_unknown(&root, &root, &known, &mut report.kept_unknown);

	if report.is_clean() {
		let _ = std::fs::remove_file(root.join(MARKER));
		let _ = std::fs::remove_dir(&root);
		let _ = std::fs::remove_dir(root.parent().unwrap_or(&root));
	} else {
		report.leftover_folder = Some(root.display().to_string());
	}

	Ok(report)
}

fn remove_file_if_unmodified(
	path: &Path,
	entry: &ManifestEntry,
	report: &mut RemovalReport,
) -> Result<(), LauncherError> {
	let Ok(metadata) = std::fs::symlink_metadata(path) else {
		return Ok(());
	};

	let unmodified =
		metadata.is_file() && file_sha256(path).is_some_and(|hash| hash == entry.sha256);

	if unmodified {
		std::fs::remove_file(path).map_err(io_error)?;
		report.removed_files += 1;
	} else {
		report.kept_modified.push(entry.path.clone());
	}

	Ok(())
}

fn file_sha256(path: &Path) -> Option<String> {
	use std::io::Read;

	let mut file = std::fs::File::open(path).ok()?;
	let mut hasher = Sha256::new();
	let mut buffer = vec![0u8; 64 * 1024];

	loop {
		let read = file.read(&mut buffer).ok()?;

		if read == 0 {
			return Some(hex::encode(hasher.finalize()));
		}

		hasher.update(&buffer[..read]);
	}
}

/// Lists files under `directory` that the manifest does not know.
fn collect_unknown(
	root: &Path,
	directory: &Path,
	known: &HashSet<String>,
	unknown: &mut Vec<String>,
) {
	let Ok(entries) = std::fs::read_dir(directory) else {
		return;
	};

	for entry in entries.flatten() {
		let path = entry.path();
		let Ok(relative) = path.strip_prefix(root) else {
			continue;
		};
		let relative = relative.to_string_lossy().replace('\\', "/");

		if relative == MARKER {
			continue;
		}

		let is_dir = entry.file_type().is_ok_and(|kind| kind.is_dir());

		if is_dir {
			collect_unknown(root, &path, known, unknown);
		} else if !known.contains(&relative) {
			unknown.push(relative);
		}
	}
}
