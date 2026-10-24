//! Checking that what is on disk is what the receipts say.
//!
//! Read-only. It compares files to the hashes recorded at install, so edits,
//! deletions, and a folder that is no longer the launcher's all show up. It
//! cannot say whether an app is safe, only whether it is as installed.

use std::collections::HashSet;
use std::path::PathBuf;

use launcher_model::{ErrorCode, InstallationId, LauncherError};
use launcher_storage::InstallationRecord;
use rayon::prelude::*;
use sha2::{Digest, Sha256};

use crate::engine::Engine;
use crate::lifecycle::OperationSummary;
use crate::lifecycle::{ManifestEntry, verify_ownership};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct VerifyReport {
	pub installation: Option<InstallationId>,
	pub name: String,
	/// The folder is where the launcher put it and carries its marker.
	pub ownership_ok: bool,
	pub files_checked: usize,
	pub missing: Vec<String>,
	pub modified: Vec<String>,
	/// Anything else worth saying.
	pub problems: Vec<String>,
}

impl VerifyReport {
	pub fn is_clean(&self) -> bool {
		self.ownership_ok
			&& self.missing.is_empty()
			&& self.modified.is_empty()
			&& self.problems.is_empty()
	}
}

#[derive(Clone, Debug)]
pub struct DoctorReport {
	pub database_ok: bool,
	pub database_detail: Vec<String>,
	pub unfinished: Vec<OperationSummary>,
	pub catalog: String,
	pub apps: Vec<VerifyReport>,
}

impl DoctorReport {
	pub fn is_healthy(&self) -> bool {
		self.database_ok
			&& self.unfinished.is_empty()
			&& self.apps.iter().all(VerifyReport::is_clean)
	}
}

impl Engine {
	/// Compares one installed app's files with its receipt.
	pub async fn verify_installation(
		&self,
		installation: InstallationId,
	) -> Result<VerifyReport, LauncherError> {
		let record = self
			.database
			.installation(installation)
			.await?
			.ok_or_else(|| {
				LauncherError::new(
					ErrorCode::InstallationNotFound,
					"That app is not installed.",
				)
			})?;
		let apps = self.data.apps_dir();

		tokio::task::spawn_blocking(move || verify_blocking(&apps, &record))
			.await
			.map_err(|error| {
				LauncherError::new(ErrorCode::InternalInvariant, "The check did not finish.")
					.with_source(error)
			})
	}

	/// A non-destructive health check of the launcher's own state.
	pub async fn doctor(&self) -> Result<DoctorReport, LauncherError> {
		let (database_ok, database_detail) = match self.database.quick_check().await {
			Ok(lines) => (lines == ["ok"], lines),
			Err(error) => (false, vec![error.user_message().to_owned()]),
		};

		let unfinished = self
			.database
			.unfinished_operations()
			.await?
			.into_iter()
			.map(OperationSummary::from)
			.collect();

		let state = self.catalog().state();
		let catalog = match (&state.catalog, &state.problem) {
			(Some(catalog), _) => format!(
				"revision {}, {} apps",
				catalog.revision(),
				catalog.project_count()
			),
			(None, Some(problem)) => problem.message.clone(),
			(None, None) => "none loaded".to_owned(),
		};

		let mut apps = Vec::new();

		for record in self.database.installations().await? {
			apps.push(self.verify_installation(record.id).await?);
		}

		Ok(DoctorReport {
			database_ok,
			database_detail,
			unfinished,
			catalog,
			apps,
		})
	}
}

fn verify_blocking(apps: &std::path::Path, record: &InstallationRecord) -> VerifyReport {
	let mut report = VerifyReport {
		installation: Some(record.id),
		name: record.display_name.clone(),
		..VerifyReport::default()
	};

	let root = match verify_ownership(apps, record) {
		Ok(root) => root,
		Err(error) => {
			report.problems.push(error.user_message().to_owned());

			return report;
		}
	};

	report.ownership_ok = true;

	let manifest: Vec<ManifestEntry> = match serde_json::from_str(&record.manifest) {
		Ok(manifest) => manifest,
		Err(_) => {
			report.problems.push("The app's record is damaged.".into());

			return report;
		}
	};

	if let Some(bad) = manifest
		.iter()
		.find(|entry| !launcher_packages::is_safe_relative_path(&entry.path))
	{
		report.problems.push(format!(
			"The record lists an unsafe path ({}).",
			bad.path.len()
		));

		return report;
	}

	// Hashing is the slow part for a big app, and files are independent.
	let results: Vec<(String, Check)> = manifest
		.par_iter()
		.filter(|entry| entry.kind == "file")
		.map(|entry| (entry.path.clone(), check_file(&root, entry)))
		.collect();

	report.files_checked = results.len();

	for (path, check) in results {
		match check {
			Check::Ok => {}
			Check::Missing => report.missing.push(path),
			Check::Modified => report.modified.push(path),
		}
	}

	report.missing.sort();
	report.modified.sort();

	// A folder or link that vanished is missing too.
	let known: HashSet<&str> = manifest.iter().map(|entry| entry.path.as_str()).collect();

	for entry in manifest.iter().filter(|entry| entry.kind != "file") {
		if std::fs::symlink_metadata(root.join(&entry.path)).is_err()
			&& known.contains(entry.path.as_str())
		{
			report.missing.push(entry.path.clone());
		}
	}

	report
}

enum Check {
	Ok,
	Missing,
	Modified,
}

fn check_file(root: &PathBuf, entry: &ManifestEntry) -> Check {
	use std::io::Read;

	let path = root.join(&entry.path);

	let Ok(mut file) = std::fs::File::open(&path) else {
		return Check::Missing;
	};

	let mut hasher = Sha256::new();
	let mut buffer = vec![0u8; 64 * 1024];

	loop {
		match file.read(&mut buffer) {
			Ok(0) => break,
			Ok(read) => hasher.update(&buffer[..read]),
			Err(_) => return Check::Modified,
		}
	}

	if hex::encode(hasher.finalize()) == entry.sha256 {
		Check::Ok
	} else {
		Check::Modified
	}
}

#[cfg(test)]
mod tests {
	use crate::{DataDirectory, Engine};

	#[tokio::test]
	async fn a_fresh_launcher_is_healthy() {
		let directory = tempfile::tempdir().unwrap();
		let engine = Engine::open(&DataDirectory::at(directory.path().join("data")))
			.await
			.unwrap();

		let report = engine.doctor().await.unwrap();

		assert!(report.database_ok);
		assert!(report.is_healthy());
		assert!(report.apps.is_empty());
	}
}
