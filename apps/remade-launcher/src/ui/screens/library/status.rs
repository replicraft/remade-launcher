//! What to tell a person about an app's update.

use launcher_core::{Decision, SkipReason, UpdateState, UpdateStatus};
use launcher_model::UpdateMode;

/// What to tell a person about an app's update, in the words the settings use.
/// A failed check is never "up to date".
pub(crate) fn status_text(status: &UpdateStatus, mode: UpdateMode) -> (String, bool) {
	match &status.state {
		UpdateState::UpToDate => ("Up to date".into(), false),
		UpdateState::Pinned => ("Pinned".into(), false),
		UpdateState::CheckFailed(why) => (
			format!("Needs your attention: could not check ({why})"),
			true,
		),
		UpdateState::Unsupported(why) => (why.clone(), false),
		UpdateState::NeedsChoice => ("Needs your attention: several packages fit".into(), true),
		UpdateState::Available => {
			let version = status
				.available
				.as_ref()
				.map_or("?", |available| available.version.as_str());

			if !mode.is_automatic() {
				return (format!("Update available: {version}"), false);
			}

			match &status.auto.decision {
				Decision::Apply => (
					format!("Update available: {version}. It will install automatically."),
					false,
				),
				Decision::Skip(SkipReason::MinReleaseAgeNotMet { .. }) => {
					(format!("Waiting for minimum age: {version}"), false)
				}
				Decision::Skip(SkipReason::AutoPaused) => (
					format!("Update available: {version}. Automatic updates are paused."),
					false,
				),
				Decision::Skip(reason) => {
					(format!("Needs your attention: {}", reason.message()), true)
				}
			}
		}
	}
}

#[cfg(test)]
mod tests {
	use launcher_core::{AutoStatus, AvailableUpdate};
	use launcher_model::{InstallationId, MinimumAge};

	use super::*;

	fn status(state: UpdateState, decision: Decision, available: bool) -> UpdateStatus {
		UpdateStatus {
			installation: InstallationId::new(),
			name: "App".into(),
			catalog_slug: Some("app".into()),
			installed_version: "v1".into(),
			state,
			available: available.then(|| AvailableUpdate {
				version: "v2".into(),
				file: "a.zip".into(),
				size: 1,
				published_at: None,
				rebuilt: false,
			}),
			auto: AutoStatus {
				mode: UpdateMode::OnLaunch,
				minimum_age: MinimumAge::DEFAULT,
				age_is_per_app: false,
				eligible_at: None,
				decision,
			},
		}
	}

	#[test]
	fn a_failed_check_is_never_called_up_to_date() {
		let failed = status(
			UpdateState::CheckFailed("offline".into()),
			Decision::Skip(SkipReason::CheckFailed("offline".into())),
			false,
		);
		let (text, attention) = status_text(&failed, UpdateMode::Manual);

		assert!(attention);
		assert!(!text.to_lowercase().contains("up to date"), "{text}");
	}

	#[test]
	fn manual_mode_offers_the_update_without_promising_to_install_it() {
		let available = status(
			UpdateState::Available,
			Decision::Skip(SkipReason::ModeManual),
			true,
		);

		assert_eq!(
			status_text(&available, UpdateMode::Manual),
			("Update available: v2".into(), false)
		);
	}

	#[test]
	fn automatic_modes_explain_what_will_happen() {
		let apply = status(UpdateState::Available, Decision::Apply, true);
		assert!(
			status_text(&apply, UpdateMode::OnLaunch)
				.0
				.contains("install automatically")
		);

		let young = status(
			UpdateState::Available,
			Decision::Skip(SkipReason::MinReleaseAgeNotMet { eligible_at: 5 }),
			true,
		);
		assert_eq!(
			status_text(&young, UpdateMode::Daemon).0,
			"Waiting for minimum age: v2"
		);

		let paused = status(
			UpdateState::Available,
			Decision::Skip(SkipReason::AutoPaused),
			true,
		);
		assert!(
			status_text(&paused, UpdateMode::OnLaunch)
				.0
				.contains("paused")
		);

		let rebuilt = status(
			UpdateState::Available,
			Decision::Skip(SkipReason::RebuiltArtifact),
			true,
		);
		let (text, attention) = status_text(&rebuilt, UpdateMode::OnLaunch);
		assert!(attention && text.starts_with("Needs your attention"));
	}

	#[test]
	fn pinned_and_current_apps_say_so() {
		assert_eq!(
			status_text(
				&status(
					UpdateState::Pinned,
					Decision::Skip(SkipReason::Pinned),
					false
				),
				UpdateMode::Manual
			)
			.0,
			"Pinned"
		);
		assert_eq!(
			status_text(
				&status(
					UpdateState::UpToDate,
					Decision::Skip(SkipReason::NotNewer),
					false
				),
				UpdateMode::Manual
			)
			.0,
			"Up to date"
		);
	}
}
