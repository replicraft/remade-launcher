//! Read-only update discovery. Nothing here installs anything, whatever the
//! update mode is.

use futures_util::StreamExt;
use launcher_model::{HostTarget, InstallationId, LauncherError, MinimumAge, UpdateMode};
use launcher_providers::{GithubProvider, Transport};
use launcher_storage::{AppPolicy, InstallationRecord, ObservedAsset};

use super::decide::{Decision, Initiator, Inputs, decide, effective_age, effective_mode};
use super::outlook::Outlook;
use crate::engine::{Engine, now_unix};

/// How many apps are looked up at once. Kept low to stay inside GitHub's
/// anonymous request allowance.
const LOOKUP_CONCURRENCY: usize = 2;

#[derive(Clone, Debug)]
pub enum UpdateState {
	UpToDate,
	/// A newer release exists, whatever the mode or the age says.
	Available,
	/// Pinned, so held at its version.
	Pinned,
	/// Cannot be checked or offered, with the reason.
	Unsupported(String),
	/// The lookup failed. Never shown as up to date.
	CheckFailed(String),
	NeedsChoice,
}

#[derive(Clone, Debug)]
pub struct AvailableUpdate {
	pub version: String,
	pub file: String,
	pub size: u64,
	pub published_at: Option<i64>,
	/// Same version label, different file.
	pub rebuilt: bool,
}

/// What would happen to this update under the current settings.
#[derive(Clone, Debug)]
pub struct AutoStatus {
	pub mode: UpdateMode,
	pub minimum_age: MinimumAge,
	/// The age came from this app's own override.
	pub age_is_per_app: bool,
	/// When an automatic update becomes allowed by age, if there is one.
	pub eligible_at: Option<i64>,
	pub decision: Decision,
}

#[derive(Clone, Debug)]
pub struct UpdateStatus {
	pub installation: InstallationId,
	pub name: String,
	pub catalog_slug: Option<String>,
	pub installed_version: String,
	pub state: UpdateState,
	pub available: Option<AvailableUpdate>,
	pub auto: AutoStatus,
}

/// An app's update status, with the plan to apply it when an operator run
/// would.
#[derive(Clone, Debug)]
pub struct OperatorUpdate {
	pub status: UpdateStatus,
	pub ready: Option<crate::prepare::Ready>,
}

/// One app after the network lookup and the policy decision.
pub(crate) struct Evaluated {
	pub(crate) record: InstallationRecord,
	pub(crate) policy: AppPolicy,
	pub(crate) outlook: Outlook,
	pub(crate) decision: Decision,
	pub(crate) auto: AutoStatus,
}

impl Engine {
	/// Looks for updates to installed apps. Read-only: it never installs.
	pub async fn check_updates(
		&self,
		only: Option<InstallationId>,
	) -> Result<Vec<UpdateStatus>, LauncherError> {
		let provider = self.provider();

		self.check_updates_with(&provider, only, HostTarget::current(), now_unix())
			.await
	}

	pub(crate) async fn check_updates_with<T: Transport>(
		&self,
		provider: &GithubProvider<T>,
		only: Option<InstallationId>,
		host: Option<HostTarget>,
		now: i64,
	) -> Result<Vec<UpdateStatus>, LauncherError> {
		let evaluated = self
			.evaluate_apps(provider, only, Initiator::Automatic, host, now)
			.await?;

		Ok(evaluated.into_iter().map(status_of).collect())
	}

	/// What an operator-initiated update run would do, as plans ready to
	/// approve. Nothing is applied here.
	pub async fn plan_operator_updates(
		&self,
		only: Option<InstallationId>,
	) -> Result<Vec<OperatorUpdate>, LauncherError> {
		let provider = self.provider();

		self.plan_operator_updates_with(&provider, only, HostTarget::current(), now_unix())
			.await
	}

	pub(crate) async fn plan_operator_updates_with<T: Transport>(
		&self,
		provider: &GithubProvider<T>,
		only: Option<InstallationId>,
		host: Option<HostTarget>,
		now: i64,
	) -> Result<Vec<OperatorUpdate>, LauncherError> {
		let evaluated = self
			.evaluate_apps(provider, only, Initiator::Operator, host, now)
			.await?;

		Ok(evaluated
			.into_iter()
			.map(|item| {
				let ready = match (&item.decision, &item.outlook) {
					(Decision::Apply, Outlook::Ready(ready)) => Some((**ready).clone()),
					_ => None,
				};

				OperatorUpdate {
					status: status_of(item),
					ready,
				}
			})
			.collect())
	}

	/// Looks up every installed app (or one) and decides, under the current
	/// settings, what an `initiator` would be allowed to do.
	pub(crate) async fn evaluate_apps<T: Transport>(
		&self,
		provider: &GithubProvider<T>,
		only: Option<InstallationId>,
		initiator: Initiator,
		host: Option<HostTarget>,
		now: i64,
	) -> Result<Vec<Evaluated>, LauncherError> {
		let settings = self.update_settings().await;
		let clock_ok = !self.catalog().state().clock_warning;

		let records: Vec<InstallationRecord> = self
			.database
			.installations()
			.await?
			.into_iter()
			.filter(|record| only.is_none_or(|id| id == record.id))
			.collect();

		// The lookups are the slow part, so a couple run together. Order is
		// kept, so output and decisions are stable.
		let looked_up: Vec<(InstallationRecord, AppPolicy, Outlook)> =
			futures_util::stream::iter(records)
				.map(|record| async move {
					let policy = self
						.database
						.app_policy(record.id)
						.await
						.unwrap_or_default();
					let outlook = self
						.outlook_with(provider, &record, &policy, host, now)
						.await;

					(record, policy, outlook)
				})
				.buffered(LOOKUP_CONCURRENCY)
				.collect()
				.await;

		let mut evaluated = Vec::with_capacity(looked_up.len());

		for (record, policy, outlook) in looked_up {
			let window_start = match &outlook {
				Outlook::Ready(ready) => Some(self.observe(ready, now).await?),
				_ => None,
			};

			let mode = effective_mode(&policy, settings.mode);

			let decision = decide(&Inputs {
				mode,
				initiator,
				curated: record.origin == "curated",
				policy: &policy,
				global_age: settings.minimum_age,
				outlook: &outlook,
				window_start,
				now,
				clock_ok,
			});

			let (minimum_age, age_is_per_app) = effective_age(&policy, settings.minimum_age);
			let eligible_at = window_start.map(|start| {
				start.saturating_add(i64::try_from(minimum_age.seconds()).unwrap_or(i64::MAX))
			});

			evaluated.push(Evaluated {
				auto: AutoStatus {
					mode,
					minimum_age,
					age_is_per_app,
					eligible_at,
					decision: decision.clone(),
				},
				record,
				policy,
				outlook,
				decision,
			});
		}

		Ok(evaluated)
	}

	/// Records the candidate asset and returns where its age clock starts.
	async fn observe(&self, ready: &crate::prepare::Ready, now: i64) -> Result<i64, LauncherError> {
		let body = ready.plan.body();
		let candidate = &ready.candidate;

		let observation = self
			.database
			.observe_asset(
				ObservedAsset {
					repository_id: body.source.repository_id,
					release_id: candidate.release.id,
					asset_id: candidate.asset.id,
					tag: candidate.release.tag.clone(),
					size: candidate.asset.size,
					host_sha256: candidate.asset.host_sha256.clone(),
					release_published_at: candidate.release.published_at,
					asset_created_at: candidate.asset.created_at,
					asset_updated_at: candidate.asset.updated_at,
				},
				now,
			)
			.await?;

		Ok(observation.window_start)
	}
}

fn status_of(item: Evaluated) -> UpdateStatus {
	let pinned = item.auto.mode == UpdateMode::Pinned;

	let (state, available) = match &item.outlook {
		Outlook::Ready(ready) => {
			let available = AvailableUpdate {
				version: ready.candidate.release.tag.clone(),
				file: ready.candidate.asset.name.clone(),
				size: ready.candidate.asset.size,
				published_at: ready.candidate.release.published_at,
				rebuilt: ready.rebuilt,
			};

			(
				if pinned {
					UpdateState::Pinned
				} else {
					UpdateState::Available
				},
				Some(available),
			)
		}
		Outlook::UpToDate => (
			if pinned {
				UpdateState::Pinned
			} else {
				UpdateState::UpToDate
			},
			None,
		),
		Outlook::Unavailable(why) => (UpdateState::Unsupported(why.clone()), None),
		Outlook::Choose(_) => (UpdateState::NeedsChoice, None),
		Outlook::Failed(why) => (UpdateState::CheckFailed(why.clone()), None),
	};

	UpdateStatus {
		installation: item.record.id,
		name: item.record.display_name,
		catalog_slug: item.record.catalog_slug,
		installed_version: item.record.release_tag,
		state,
		available,
		auto: item.auto,
	}
}
