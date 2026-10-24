//! Finishing or undoing operations a crash left behind.
//!
//! The journal is the authority. Each unfinished operation is classified by
//! the last state it reached, and recovery only removes what it can prove
//! the launcher created.

use launcher_model::{LauncherError, OperationId};
use launcher_storage::OperationRecord;

use super::install::{LifecycleService, state};
use super::remove::verify_ownership;
use super::safe_delete::remove_staging;

/// What recovery did, for the log and for tests.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RecoveryReport {
	pub rolled_back: usize,
	pub completed: usize,
	pub needs_attention: usize,
}

impl LifecycleService {
	/// Resolves every unfinished operation. Safe to run on every start.
	pub async fn recover(&self, now: i64) -> Result<RecoveryReport, LauncherError> {
		let mut report = RecoveryReport::default();

		// If another process holds the lock, its operations are alive, not
		// crashed. Leave them alone and try again at the next start.
		let Some(_lock) = super::lock::try_acquire(&self.data) else {
			tracing::info!("skipping recovery: another operation is running");

			return Ok(report);
		};

		for operation in self.database.unfinished_operations().await? {
			match self.recover_one(&operation, now).await {
				Ok(Outcome::RolledBack) => report.rolled_back += 1,
				Ok(Outcome::Completed) => report.completed += 1,
				Ok(Outcome::NeedsAttention) => report.needs_attention += 1,
				Err(error) => {
					tracing::warn!(%error, operation = %operation.id, "recovery could not finish an operation");
					report.needs_attention += 1;
				}
			}
		}

		Ok(report)
	}

	async fn recover_one(
		&self,
		operation: &OperationRecord,
		now: i64,
	) -> Result<Outcome, LauncherError> {
		match (operation.kind.as_str(), operation.state.as_str()) {
			// Nothing outside staging had changed. Discard the staging folder.
			(
				"install" | "update",
				"accepted" | "downloading" | "verifying" | "inspecting" | "extracting",
			) => {
				self.discard_staging(operation.id);
				self.mark(
					operation.id,
					state::FAILED,
					"Interrupted before anything was installed; staging was removed.",
					now,
				)
				.await?;

				Ok(Outcome::RolledBack)
			}
			("install" | "update", "applying") => self.recover_applying(operation, now).await,
			// Removal is idempotent: running it again finishes the job.
			("uninstall", _) => self.recover_removal(operation, now).await,
			_ => {
				self.mark(
					operation.id,
					state::RECOVERY_REQUIRED,
					"The launcher found an operation it could not classify.",
					now,
				)
				.await?;

				Ok(Outcome::NeedsAttention)
			}
		}
	}

	/// The files may or may not have been moved into place, and the receipt
	/// may or may not have been written.
	async fn recover_applying(
		&self,
		operation: &OperationRecord,
		now: i64,
	) -> Result<Outcome, LauncherError> {
		let Some(installation) = operation.installation_id else {
			return Ok(Outcome::NeedsAttention);
		};
		let root = operation.detail.clone().map(std::path::PathBuf::from);
		let receipt = self.database.installation(installation).await?;

		// The receipt points at this operation's folder: it committed, but the
		// journal write was lost. Finish it.
		if let (Some(receipt), Some(root)) = (&receipt, &root)
			&& std::path::Path::new(&receipt.root) == root
		{
			self.mark(operation.id, "completed", "Finished by recovery.", now)
				.await?;
			self.discard_staging(operation.id);

			return Ok(Outcome::Completed);
		}

		// No receipt: the install never became real. Remove the folder only
		// if its marker names this operation.
		if let Some(root) = root
			&& marker_names_operation(&root, operation.id)
		{
			let _ = std::fs::remove_dir_all(&root);

			if let Some(parent) = root.parent() {
				let _ = std::fs::remove_dir(parent);
			}
		}

		self.discard_staging(operation.id);
		self.mark(
			operation.id,
			state::FAILED,
			"Rolled back after an interruption.",
			now,
		)
		.await?;

		Ok(Outcome::RolledBack)
	}

	async fn recover_removal(
		&self,
		operation: &OperationRecord,
		now: i64,
	) -> Result<Outcome, LauncherError> {
		let Some(installation) = operation.installation_id else {
			return Ok(Outcome::NeedsAttention);
		};

		match self.database.installation(installation).await? {
			// The receipt is already gone, so the removal had committed.
			None => {
				self.mark(operation.id, "completed", "Finished by recovery.", now)
					.await?;

				Ok(Outcome::Completed)
			}
			Some(record) => {
				if verify_ownership(&self.data.apps_dir(), &record).is_err()
					&& std::path::Path::new(&record.root).exists()
				{
					self.mark(
						operation.id,
						state::RECOVERY_REQUIRED,
						"The app's folder changed during removal.",
						now,
					)
					.await?;

					return Ok(Outcome::NeedsAttention);
				}

				self.remove_installed_files(&record).await?;
				self.database
					.commit_removal(
						operation.id,
						installation,
						"Finished by recovery.".into(),
						now,
					)
					.await?;

				Ok(Outcome::Completed)
			}
		}
	}

	fn discard_staging(&self, operation: OperationId) {
		if let Err(error) = remove_staging(&self.data.staging_dir(), &operation.to_string()) {
			tracing::warn!(%error, "could not remove staging during recovery");
		}
	}

	async fn mark(
		&self,
		id: OperationId,
		to: &'static str,
		detail: &str,
		now: i64,
	) -> Result<(), LauncherError> {
		self.database
			.set_operation_state(id, to, None, Some(detail.to_owned()), now)
			.await
	}
}

enum Outcome {
	RolledBack,
	Completed,
	NeedsAttention,
}

fn marker_names_operation(root: &std::path::Path, operation: OperationId) -> bool {
	std::fs::read_to_string(root.join(super::safe_delete::MARKER))
		.ok()
		.and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
		.and_then(|value| value["operation"].as_str().map(str::to_owned))
		.is_some_and(|recorded| recorded == operation.to_string())
}
