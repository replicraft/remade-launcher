//! One automatic-update cycle.

use launcher_model::{HostTarget, InstallationId, LauncherError, OperationId, UpdateMode};
use launcher_providers::{GithubProvider, Transport};
use launcher_resolver::Consent;
use launcher_storage::{ConsentRecord, DecisionRecord};

use super::decide::{Decision, Initiator, SkipReason};
use super::outlook::Outlook;
use super::status::Evaluated;
use crate::engine::{Engine, now_unix};

/// What started a cycle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Trigger {
	/// A new launcher process started in an automatic mode.
	OnLaunch,
	/// The background updater's own schedule.
	Daemon,
	/// A person asked for an update run. It ignores the mode.
	Operator,
}

impl Trigger {
	pub const fn as_str(self) -> &'static str {
		match self {
			Self::OnLaunch => "on-launch",
			Self::Daemon => "daemon",
			Self::Operator => "operator",
		}
	}

	const fn initiator(self) -> Initiator {
		match self {
			Self::OnLaunch | Self::Daemon => Initiator::Automatic,
			Self::Operator => Initiator::Operator,
		}
	}
}

#[derive(Clone, Debug)]
pub enum CycleResult {
	Updated { to: String },
	Skipped(SkipReason),
	Failed(String),
}

#[derive(Clone, Debug)]
pub struct CycleOutcome {
	pub installation: InstallationId,
	pub name: String,
	pub from: String,
	pub result: CycleResult,
}

#[derive(Clone, Debug)]
pub struct CycleReport {
	pub cycle_id: String,
	pub trigger: Trigger,
	pub started_at: i64,
	/// Another process was already running a cycle, so this one did nothing.
	pub already_running: bool,
	/// The mode does not allow this trigger to install anything.
	pub mode_blocked: bool,
	pub outcomes: Vec<CycleOutcome>,
}

impl CycleReport {
	pub fn updated(&self) -> usize {
		self.outcomes
			.iter()
			.filter(|o| matches!(o.result, CycleResult::Updated { .. }))
			.count()
	}

	pub fn failed(&self) -> usize {
		self.outcomes
			.iter()
			.filter(|o| matches!(o.result, CycleResult::Failed(_)))
			.count()
	}
}

impl Engine {
	/// Runs one cycle with the real provider.
	pub async fn run_update_cycle(&self, trigger: Trigger) -> Result<CycleReport, LauncherError> {
		let provider = GithubProvider::new(self.http.clone());

		self.run_update_cycle_with(&provider, trigger, HostTarget::current(), now_unix())
			.await
	}

	pub(crate) async fn run_update_cycle_with<T: Transport>(
		&self,
		provider: &GithubProvider<T>,
		trigger: Trigger,
		host: Option<HostTarget>,
		now: i64,
	) -> Result<CycleReport, LauncherError> {
		let mut report = CycleReport {
			cycle_id: OperationId::new().to_string(),
			trigger,
			started_at: now,
			already_running: false,
			mode_blocked: false,
			outcomes: Vec::new(),
		};

		let settings = self.update_settings().await;

		if trigger.initiator() == Initiator::Automatic && settings.mode == UpdateMode::Manual {
			report.mode_blocked = true;

			return Ok(report);
		}

		// One cycle at a time across processes. A second one joins nothing and
		// does nothing, which is how a GUI start and the daemon coalesce.
		let Some(_cycle) = crate::lifecycle::cycle_lock(&self.data) else {
			report.already_running = true;

			return Ok(report);
		};

		self.database
			.set_setting("updates.last_cycle_started", now.to_string())
			.await?;

		let evaluated = self
			.evaluate_apps(provider, None, trigger.initiator(), host, now)
			.await?;

		for item in evaluated {
			let outcome = self.settle(&report.cycle_id, trigger, item, now).await;

			report.outcomes.push(outcome);
		}

		self.database
			.set_setting("updates.last_cycle_completed", now.to_string())
			.await?;
		self.database
			.set_setting("updates.last_cycle_id", report.cycle_id.clone())
			.await?;

		Ok(report)
	}

	/// Applies or skips one app, and records why.
	pub(crate) async fn settle(
		&self,
		cycle_id: &str,
		trigger: Trigger,
		item: Evaluated,
		now: i64,
	) -> CycleOutcome {
		let Evaluated {
			record,
			policy,
			outlook,
			decision,
			auto,
		} = item;

		let policy_snapshot = serde_json::json!({
			"mode": auto.mode.as_str(),
			"minimum_age_seconds": auto.minimum_age.seconds(),
			"per_app_age": auto.age_is_per_app,
			"auto_paused": policy.auto_paused,
			"pinned": policy.pinned_release_id.is_some(),
		})
		.to_string();

		let (candidate_release, candidate_asset, plan_digest) = match &outlook {
			Outlook::Ready(ready) => (
				Some(ready.candidate.release.id),
				Some(ready.candidate.asset.id),
				Some(ready.plan.digest()),
			),
			_ => (None, None, None),
		};

		let record_decision = |outcome: &str, reason: String| DecisionRecord {
			installation_id: record.id,
			cycle_id: cycle_id.to_owned(),
			trigger: trigger.as_str().to_owned(),
			candidate_release_id: candidate_release,
			candidate_asset_id: candidate_asset,
			outcome: outcome.to_owned(),
			reason,
			policy: policy_snapshot.clone(),
			plan_digest,
			decided_at: now,
		};

		// The mode can change while a pass is running. A person who switches to
		// Manual must not see another install start afterwards.
		let decision = match decision {
			Decision::Apply
				if trigger.initiator() == Initiator::Automatic
					&& self.update_settings().await.mode == UpdateMode::Manual =>
			{
				Decision::Skip(SkipReason::ModeManual)
			}
			other => other,
		};

		let result = match (decision, outlook) {
			(Decision::Skip(reason), _) => {
				let _ = self
					.database
					.record_decision(record_decision("skipped", reason.code().into()))
					.await;

				CycleResult::Skipped(reason)
			}
			(Decision::Apply, Outlook::Ready(ready)) => {
				match self
					.apply_automatically(&record.id, &ready, trigger, now)
					.await
				{
					Ok(()) => {
						let _ = self
							.database
							.record_decision(record_decision("applied", "UPDATED".into()))
							.await;

						CycleResult::Updated {
							to: ready.candidate.release.tag.clone(),
						}
					}
					Err(error) => {
						tracing::warn!(%error, app = %record.display_name, "an automatic update failed");
						let _ = self
							.database
							.record_decision(record_decision(
								"failed",
								error.code().as_str().into(),
							))
							.await;

						CycleResult::Failed(error.user_message().to_owned())
					}
				}
			}
			// `decide` only applies a ready update, so this is unreachable.
			(Decision::Apply, _) => CycleResult::Failed("There was nothing to apply.".into()),
		};

		CycleOutcome {
			installation: record.id,
			name: record.display_name,
			from: record.release_tag,
			result,
		}
	}

	/// Approves the plan on the user's behalf, under the standing consent
	/// they gave for the mode, and records that approval.
	async fn apply_automatically(
		&self,
		installation: &InstallationId,
		ready: &crate::prepare::Ready,
		trigger: Trigger,
		now: i64,
	) -> Result<(), LauncherError> {
		let consent = Consent {
			source_shown: ready.source_display.clone(),
			acknowledged_unreviewed: ready.unreviewed,
		};
		let approved = ready.plan.clone().approve(&consent, now).map_err(|error| {
			LauncherError::new(
				launcher_model::ErrorCode::PlanExpired,
				"The update plan was no longer valid.",
			)
			.with_source(error)
		})?;

		self.database
			.record_consent(ConsentRecord {
				kind: "plan".into(),
				subject: hex_digest(&ready.plan.digest()),
				scope: ready.source_display.clone(),
				policy_version: launcher_model::CONSENT_POLICY_VERSION,
				actor: format!("automatic:{}", trigger.as_str()),
				granted_at: now,
			})
			.await?;

		self.lifecycle()
			.update(*installation, &approved, &ready.meta, now)
			.await
			.map(drop)
	}
}

fn hex_digest(digest: &[u8; 32]) -> String {
	digest.iter().map(|byte| format!("{byte:02x}")).collect()
}
