//! What an installed app's update looks like right now.

use launcher_catalog::Channel;
use launcher_model::HostTarget;
use launcher_providers::{GithubProvider, Transport};
use launcher_resolver::{Candidate, Recency, compare_tags};
use launcher_storage::{AppPolicy, InstallationRecord};

use crate::engine::Engine;
use crate::prepare::{Preparation, Ready};

/// The result of looking for an update, with the reasons kept typed so the
/// decision and the screens never have to read prose.
#[derive(Clone, Debug)]
pub enum Outlook {
	/// A newer, compatible, verifiable release, as a plan ready to approve.
	Ready(Box<Ready>),
	UpToDate,
	/// Nothing can be offered, and why.
	Unavailable(String),
	/// Several packages fit equally. The launcher does not pick.
	Choose(Vec<Candidate>),
	/// The lookup itself failed. This is never reported as "up to date".
	Failed(String),
}

impl Engine {
	pub(crate) async fn outlook_with<T: Transport>(
		&self,
		provider: &GithubProvider<T>,
		record: &InstallationRecord,
		policy: &AppPolicy,
		host: Option<HostTarget>,
		now: i64,
	) -> Outlook {
		let Some(slug) = record
			.catalog_slug
			.clone()
			.filter(|_| record.origin == "curated")
		else {
			return Outlook::Unavailable(
				"You added this app yourself. Install a newer release from its source to update it.".into(),
			);
		};

		// A prerelease opt-in widens a signed stable policy and nothing else.
		let channel = policy.allow_prerelease.then_some(Channel::Beta);

		let preparation = self
			.prepare_curated_on(
				provider,
				&slug,
				Some((record.id, &record.variant_id)),
				channel,
				host,
				now,
			)
			.await;

		let mut ready = match preparation {
			Err(error) => return Outlook::Failed(error.user_message().to_owned()),
			Ok(Preparation::Unavailable(why)) => return Outlook::Unavailable(why),
			Ok(Preparation::Choose(candidates)) => return Outlook::Choose(candidates),
			Ok(Preparation::Ready(ready)) => ready,
		};

		let same_release = ready.candidate.release.id == record.release_id;

		if same_release && ready.candidate.asset.id == record.asset_id {
			return Outlook::UpToDate;
		}

		let catalog = self.catalog().state().catalog.clone();
		let project = catalog.as_ref().and_then(|catalog| catalog.project(&slug));
		let Some(project) = project else {
			return Outlook::Unavailable("That app is no longer in the catalog.".into());
		};

		match compare_tags(project, &record.release_tag, &ready.candidate.release.tag) {
			Recency::Newer => Outlook::Ready(ready),
			Recency::Older => Outlook::UpToDate,
			Recency::Same => {
				// The same version label with a different file: a rebuilt
				// artifact. It is offered, but never applied unattended.
				ready.rebuilt = true;
				ready
					.notes
					.push("The publisher replaced the file for the version you have.".into());

				Outlook::Ready(ready)
			}
			Recency::Unordered => {
				let opaque = project.release_policy.version_strategy
					== launcher_catalog::VersionStrategy::Opaque;
				let newer_in_time = ready
					.candidate
					.release
					.published_at
					.is_some_and(|at| at > record.installed_at);

				if opaque && newer_in_time {
					Outlook::Ready(ready)
				} else if opaque {
					Outlook::UpToDate
				} else {
					Outlook::Unavailable(format!(
						"Cannot tell whether {} is newer than {}.",
						ready.candidate.release.tag, record.release_tag
					))
				}
			}
		}
	}
}
