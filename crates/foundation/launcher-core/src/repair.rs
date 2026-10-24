//! Repairing an installed app: first look, then put it back.
//!
//! The first step only looks. It compares the files with the receipt and says
//! what is wrong. Putting the app back is a plan like any other: it fetches
//! the release again, checks it under today's rules, and replaces the app's
//! own files. Files the person edited and data the app saved are kept, and
//! nothing from an old cache is trusted.

use launcher_model::{ErrorCode, HostTarget, InstallationId, LauncherError};
use launcher_providers::{GithubProvider, Transport};

use crate::engine::{Engine, now_unix};
use crate::inspect::VerifyReport;
use crate::prepare::{Preparation, Ready};

/// What a repair would do.
#[derive(Debug)]
pub enum RepairOutcome {
	/// The files match the receipt.
	NotNeeded,
	/// A plan to approve.
	Ready(Box<Ready>),
	/// Why a repair cannot be planned, and what to do instead.
	Unavailable(String),
}

#[derive(Debug)]
pub struct RepairAssessment {
	pub report: VerifyReport,
	pub outcome: RepairOutcome,
}

impl Engine {
	/// Looks at one installed app and, if it is damaged, plans putting it
	/// back. Changes nothing.
	pub async fn assess_repair(
		&self,
		installation: InstallationId,
	) -> Result<RepairAssessment, LauncherError> {
		let provider = self.provider();

		self.assess_repair_with(&provider, installation, HostTarget::current(), now_unix())
			.await
	}

	pub(crate) async fn assess_repair_with<T: Transport>(
		&self,
		provider: &GithubProvider<T>,
		installation: InstallationId,
		host: Option<HostTarget>,
		now: i64,
	) -> Result<RepairAssessment, LauncherError> {
		let report = self.verify_installation(installation).await?;

		if report.is_clean() {
			return Ok(RepairAssessment {
				report,
				outcome: RepairOutcome::NotNeeded,
			});
		}

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

		// A folder that is no longer ours is not repaired by writing into it.
		if !report.ownership_ok {
			return Ok(RepairAssessment {
				report,
				outcome: RepairOutcome::Unavailable(
					"This folder is no longer marked as the launcher's, so it will not be written to. Use `remade forget` to stop managing it, then install again.".into(),
				),
			});
		}

		let Some(slug) = record
			.catalog_slug
			.clone()
			.filter(|_| record.origin == "curated")
		else {
			return Ok(RepairAssessment {
				report,
				outcome: RepairOutcome::Unavailable(
					"This app came from a repository you chose, so there is no catalog entry to repair it from. Install it again from its source.".into(),
				),
			});
		};

		let preparation = self
			.prepare_curated(
				provider,
				&slug,
				Some((installation, &record.variant_id)),
				host,
				now,
			)
			.await?;

		let outcome = match preparation {
			Preparation::Ready(mut ready) => {
				let pinned = self
					.database
					.app_policy(installation)
					.await?
					.pinned_release_id;

				if ready.candidate.release.id != record.release_id {
					// The newest eligible release is not the one that is
					// installed. For a pinned app that would be an upgrade by
					// the back door, so it is refused.
					if pinned.is_some() {
						return Ok(RepairAssessment {
							report,
							outcome: RepairOutcome::Unavailable(format!(
								"{} is pinned to {}, and the newest release is {}. Unpin it, repair it, then pin it again.",
								record.display_name,
								record.release_tag,
								ready.candidate.release.tag
							)),
						});
					}

					ready.notes.push(format!(
						"The newest release is {}, so repairing also updates you from {}.",
						ready.candidate.release.tag, record.release_tag
					));
				}

				ready.notes.push(
					"Files you edited and data the app saved are kept. Only files the launcher installed are replaced.".into(),
				);

				RepairOutcome::Ready(ready)
			}
			Preparation::Choose(_) => RepairOutcome::Unavailable(
				"Several packages fit this device, and the launcher will not guess which you want."
					.into(),
			),
			Preparation::Unavailable(why) => RepairOutcome::Unavailable(why),
		};

		Ok(RepairAssessment { report, outcome })
	}
}
