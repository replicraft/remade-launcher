//! What to tell a person about an app's update.

use launcher_core::{Decision, SkipReason, UpdateState, UpdateStatus};
use launcher_model::UpdateMode;

/// What to tell a person about an app's update, in the words the settings use.
/// A failed check is never "up to date".
pub(crate) fn status_text(status: &UpdateStatus, mode: UpdateMode) -> (String, bool) {
	match &status.state {
		UpdateState::UpToDate => (t!("library-status-up-to-date"), false),
		UpdateState::Pinned => (t!("library-status-pinned"), false),
		UpdateState::CheckFailed(why) => {
			(t!("library-status-check-failed", why = why.as_str()), true)
		}
		UpdateState::Unsupported(why) => (why.clone(), false),
		UpdateState::NeedsChoice => (t!("library-status-needs-choice"), true),
		UpdateState::Available => {
			let version = status
				.available
				.as_ref()
				.map_or("?", |available| available.version.as_str());

			if !mode.is_automatic() {
				return (
					t!("library-status-update-available", version = version),
					false,
				);
			}

			match &status.auto.decision {
				Decision::Apply => (t!("library-status-update-auto", version = version), false),
				Decision::Skip(SkipReason::MinReleaseAgeNotMet { .. }) => {
					(t!("library-status-waiting-age", version = version), false)
				}
				Decision::Skip(SkipReason::AutoPaused) => {
					(t!("library-status-update-paused", version = version), false)
				}
				Decision::Skip(reason) => (
					t!("library-status-attention", reason = reason.message()),
					true,
				),
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
