//! The updater's link to the engine.

use std::fs::{File, OpenOptions, TryLockError};
use std::sync::Arc;

use launcher_background::schedule::SavedSchedule;
use launcher_background::{Backend, BackgroundPaths, BoxFuture, CycleSummary};
use launcher_core::{CycleResult, Engine, SkipReason, Trigger};
use launcher_model::UpdateMode;

const LAST_ATTEMPTED: &str = "daemon.last_attempted";
const LAST_COMPLETED: &str = "daemon.last_completed";
const NEXT_DUE: &str = "daemon.next_due";
const LAST_OUTCOME: &str = "daemon.last_outcome";

/// Takes the one-updater-per-user lock. Dropping the returned file frees it.
pub(crate) fn take_lock(paths: &BackgroundPaths) -> Option<File> {
	std::fs::create_dir_all(paths.folder()).ok()?;

	let file = OpenOptions::new()
		.create(true)
		.truncate(false)
		.write(true)
		.open(&paths.lock)
		.ok()?;

	match file.try_lock() {
		Ok(()) => Some(file),
		Err(TryLockError::WouldBlock | TryLockError::Error(_)) => None,
	}
}

pub(crate) struct EngineBackend {
	engine: Arc<Engine>,
}

impl EngineBackend {
	pub(crate) fn new(engine: Arc<Engine>) -> Self {
		Self { engine }
	}

	async fn number(&self, key: &'static str) -> Option<i64> {
		self.engine.state_value(key).await?.parse().ok()
	}
}

impl Backend for EngineBackend {
	fn mode_is_background(&self) -> BoxFuture<'_, bool> {
		Box::pin(async move { self.engine.update_settings().await.mode == UpdateMode::Daemon })
	}

	fn load_schedule(&self) -> BoxFuture<'_, SavedSchedule> {
		Box::pin(async move {
			SavedSchedule {
				last_attempted: self.number(LAST_ATTEMPTED).await,
				last_completed: self.number(LAST_COMPLETED).await,
				next_due: self.number(NEXT_DUE).await,
			}
		})
	}

	fn save_schedule(&self, schedule: SavedSchedule, outcome: Option<String>) -> BoxFuture<'_, ()> {
		Box::pin(async move {
			for (key, value) in [
				(LAST_ATTEMPTED, schedule.last_attempted),
				(LAST_COMPLETED, schedule.last_completed),
				(NEXT_DUE, schedule.next_due),
			] {
				if let Some(value) = value
					&& let Err(error) = self.engine.set_state_value(key, value.to_string()).await
				{
					tracing::warn!(%error, "could not save the updater's schedule");
				}
			}

			if let Some(outcome) = outcome {
				let _ = self.engine.set_state_value(LAST_OUTCOME, outcome).await;
			}
		})
	}

	fn scan(&self) -> BoxFuture<'_, CycleSummary> {
		Box::pin(async move {
			// A fresh catalog first, so a revocation or a new revision is seen.
			self.engine.catalog().refresh().await;

			let report = match self.engine.run_update_cycle(Trigger::Daemon).await {
				Ok(report) => report,
				Err(error) => {
					tracing::warn!(%error, "the scan could not run");

					return CycleSummary {
						failed: 1,
						lookups_failed: true,
						detail: error.user_message().to_owned(),
						..CycleSummary::default()
					};
				}
			};

			let mut summary = CycleSummary {
				busy: report.already_running,
				..CycleSummary::default()
			};

			for outcome in &report.outcomes {
				match &outcome.result {
					CycleResult::Updated { to } => {
						summary.updated += 1;
						tracing::info!(app = %outcome.name, from = %outcome.from, %to, "updated an app");
					}
					CycleResult::Skipped(reason) => {
						summary.skipped += 1;
						tracing::info!(app = %outcome.name, reason = reason.code(), "skipped an app");
					}
					CycleResult::Failed(message) => {
						summary.failed += 1;
						tracing::warn!(app = %outcome.name, %message, "an update failed");
					}
				}
			}

			summary.lookups_failed = !report.outcomes.is_empty()
				&& report.outcomes.iter().all(|outcome| {
					matches!(
						&outcome.result,
						CycleResult::Skipped(SkipReason::CheckFailed(_))
					)
				});

			tracing::info!(
				updated = summary.updated,
				skipped = summary.skipped,
				failed = summary.failed,
				"a scan finished"
			);

			summary
		})
	}
}
