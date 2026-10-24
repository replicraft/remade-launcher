//! Whether an automatic update may be applied.
//!
//! A pure function of what is known, with the rules in the order the
//! specification gives them. Every refusal has a stable code, so a skipped
//! update always says why, and none of them is silently overridden.

use launcher_model::{MinimumAge, UpdateMode};
use launcher_storage::AppPolicy;

use super::outlook::Outlook;

/// Why an update was not applied.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SkipReason {
	/// The global mode is Manual, so nothing installs on its own.
	ModeManual,
	/// The app is pinned, which no automatic or operator update overrides.
	Pinned,
	/// Automatic updates are paused for this app.
	AutoPaused,
	/// A repository Remade has not reviewed was never opted in to unattended
	/// updates.
	SourceNotEnrolled,
	/// Nothing newer than what is installed.
	NotNewer,
	/// Same version label, different file. A person should look first.
	RebuiltArtifact,
	/// The release is younger than the minimum age.
	MinReleaseAgeNotMet { eligible_at: i64 },
	/// The clock cannot be trusted enough to judge age.
	ClockUncertain,
	/// No compatible package, an unsupported type, or a catalog problem.
	Unavailable(String),
	/// Several packages fit, and the launcher will not choose.
	Ambiguous,
	/// Looking for the update failed.
	CheckFailed(String),
}

impl SkipReason {
	/// The stable code reported in JSON and recorded in the decision history.
	pub fn code(&self) -> &'static str {
		match self {
			Self::ModeManual => "MODE_MANUAL",
			Self::Pinned => "PINNED",
			Self::AutoPaused => "AUTO_PAUSED",
			Self::SourceNotEnrolled => "UNVERIFIED_SOURCE_APPROVAL_REQUIRED",
			Self::NotNewer => "NOT_NEWER",
			Self::RebuiltArtifact => "REBUILT_ARTIFACT",
			Self::MinReleaseAgeNotMet { .. } => "MIN_RELEASE_AGE_NOT_MET",
			Self::ClockUncertain => "CLOCK_UNCERTAIN",
			Self::Unavailable(_) => "NO_COMPATIBLE_UPDATE",
			Self::Ambiguous => "ASSET_AMBIGUOUS",
			Self::CheckFailed(_) => "RELEASE_LOOKUP_FAILED",
		}
	}

	/// A short stable word for status displays and JSON.
	pub fn status(&self) -> &'static str {
		match self {
			Self::ModeManual => "manual_mode",
			Self::Pinned => "pinned",
			Self::AutoPaused => "auto_paused",
			Self::SourceNotEnrolled => "source_not_enrolled",
			Self::NotNewer => "up_to_date",
			Self::RebuiltArtifact => "rebuilt_artifact",
			Self::MinReleaseAgeNotMet { .. } => "waiting_for_release_age",
			Self::ClockUncertain => "clock_uncertain",
			Self::Unavailable(_) => "unavailable",
			Self::Ambiguous => "ambiguous",
			Self::CheckFailed(_) => "check_failed",
		}
	}

	/// A sentence for a person.
	pub fn message(&self) -> String {
		match self {
			Self::ModeManual => "Automatic updates are off. Update it yourself when you like.".into(),
			Self::Pinned => "It is pinned to its current version. Unpin it to update.".into(),
			Self::AutoPaused => "Automatic updates are paused for this app.".into(),
			Self::SourceNotEnrolled => {
				"Remade has not reviewed this source, so it is not updated unattended.".into()
			}
			Self::NotNewer => "It is up to date.".into(),
			Self::RebuiltArtifact => {
				"The publisher replaced the file of the version you have. Review it before updating.".into()
			}
			Self::MinReleaseAgeNotMet { .. } => "The new release is not old enough yet.".into(),
			Self::ClockUncertain => {
				"The computer's clock looks wrong, so release age cannot be judged.".into()
			}
			Self::Unavailable(why) | Self::CheckFailed(why) => why.clone(),
			Self::Ambiguous => "More than one package fits, so none was chosen.".into(),
		}
	}
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Decision {
	Apply,
	Skip(SkipReason),
}

/// Who is asking. An operator run is a person's deliberate request, so it is
/// not held back by the mode, a pause, or the minimum age. It still honors a
/// pin, and still needs a verified, compatible, newer release.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Initiator {
	Automatic,
	Operator,
}

pub struct Inputs<'a> {
	pub mode: UpdateMode,
	pub initiator: Initiator,
	pub curated: bool,
	pub policy: &'a AppPolicy,
	pub global_age: MinimumAge,
	pub outlook: &'a Outlook,
	/// Where the candidate asset's age clock starts, if there is a candidate.
	pub window_start: Option<i64>,
	pub now: i64,
	pub clock_ok: bool,
}

/// The age this app waits, and whether that came from the app itself.
pub fn effective_age(policy: &AppPolicy, global: MinimumAge) -> (MinimumAge, bool) {
	match policy.min_age_seconds.and_then(MinimumAge::from_seconds) {
		Some(age) => (age, true),
		None => (global, false),
	}
}

pub fn decide(input: &Inputs<'_>) -> Decision {
	let automatic = input.initiator == Initiator::Automatic;

	if automatic && !input.mode.is_automatic() {
		return Decision::Skip(SkipReason::ModeManual);
	}

	if input.policy.pinned_release_id.is_some() {
		return Decision::Skip(SkipReason::Pinned);
	}

	if automatic && input.policy.auto_paused {
		return Decision::Skip(SkipReason::AutoPaused);
	}

	if automatic && !input.curated && !input.policy.unattended_custom {
		return Decision::Skip(SkipReason::SourceNotEnrolled);
	}

	let ready = match input.outlook {
		Outlook::Ready(ready) => ready,
		Outlook::UpToDate => return Decision::Skip(SkipReason::NotNewer),
		Outlook::Unavailable(why) => return Decision::Skip(SkipReason::Unavailable(why.clone())),
		Outlook::Choose(_) => return Decision::Skip(SkipReason::Ambiguous),
		Outlook::Failed(why) => return Decision::Skip(SkipReason::CheckFailed(why.clone())),
	};

	if automatic && ready.rebuilt {
		return Decision::Skip(SkipReason::RebuiltArtifact);
	}

	if automatic {
		if !input.clock_ok {
			return Decision::Skip(SkipReason::ClockUncertain);
		}

		let (age, _) = effective_age(input.policy, input.global_age);
		let start = input.window_start.unwrap_or(input.now);
		let eligible_at = start.saturating_add(i64::try_from(age.seconds()).unwrap_or(i64::MAX));

		if input.now < eligible_at {
			return Decision::Skip(SkipReason::MinReleaseAgeNotMet { eligible_at });
		}
	}

	Decision::Apply
}

#[cfg(test)]
mod tests {
	use launcher_resolver::{
		AssetRef, Candidate, EvidenceLevel, InstallBinding, Operation, Origin, Plan, ReleaseRef,
		SourceBinding,
	};

	use super::*;
	use crate::lifecycle::InstallMeta;
	use crate::prepare::Ready;

	fn ready(rebuilt: bool) -> Ready {
		let candidate = Candidate {
			release: ReleaseRef {
				id: 2,
				tag: "v2".into(),
				page_url: String::new(),
				prerelease: false,
				order: launcher_resolver::OrderBasis::Defined,
				published_at: Some(1_000),
			},
			variant_id: "v".into(),
			asset: AssetRef {
				id: 9,
				name: "a.zip".into(),
				size: 1,
				download_url: "https://github.com/o/r/a.zip".into(),
				host_sha256: None,
				created_at: None,
				updated_at: None,
			},
			explanation: Vec::new(),
			skipped_newer: Vec::new(),
		};
		let plan = Plan::for_candidate(
			Operation::Update,
			Origin::Custom,
			SourceBinding {
				repository_id: 1,
				owner_id: 2,
				display: "o/r".into(),
			},
			&candidate,
			InstallBinding {
				variant_id: "v".into(),
				adapter: "portable-v1".into(),
				scope: "user".into(),
				format: "zip".into(),
				entry_point: None,
				strip_components: 0,
				evidence: EvidenceLevel::HostDigest,
				expected_sha256: None,
			},
			0,
		)
		.unwrap();

		Ready {
			plan,
			candidate,
			meta: InstallMeta {
				display_name: "App".into(),
				catalog_slug: None,
			},
			source_display: "o/r".into(),
			unreviewed: false,
			notes: Vec::new(),
			rebuilt,
		}
	}

	const NOW: i64 = 100_000;

	struct Case {
		mode: UpdateMode,
		initiator: Initiator,
		curated: bool,
		policy: AppPolicy,
		outlook: Outlook,
		window_start: Option<i64>,
		clock_ok: bool,
	}

	impl Case {
		fn new() -> Self {
			Self {
				mode: UpdateMode::OnLaunch,
				initiator: Initiator::Automatic,
				curated: true,
				policy: AppPolicy::default(),
				outlook: Outlook::Ready(Box::new(ready(false))),
				window_start: Some(NOW - 7_200),
				clock_ok: true,
			}
		}

		fn decide(&self) -> Decision {
			decide(&Inputs {
				mode: self.mode,
				initiator: self.initiator,
				curated: self.curated,
				policy: &self.policy,
				global_age: MinimumAge::DEFAULT,
				outlook: &self.outlook,
				window_start: self.window_start,
				now: NOW,
				clock_ok: self.clock_ok,
			})
		}
	}

	fn skipped(case: &Case) -> SkipReason {
		match case.decide() {
			Decision::Skip(reason) => reason,
			Decision::Apply => panic!("expected a skip"),
		}
	}

	#[test]
	fn a_mature_eligible_update_applies() {
		assert_eq!(Case::new().decide(), Decision::Apply);
	}

	#[test]
	fn manual_mode_never_installs_on_its_own() {
		let mut case = Case::new();
		case.mode = UpdateMode::Manual;

		assert_eq!(skipped(&case), SkipReason::ModeManual);
	}

	#[test]
	fn the_age_rule_uses_the_window_and_the_effective_age() {
		let mut case = Case::new();

		// One hour default; released 20 minutes ago.
		case.window_start = Some(NOW - 1_200);
		assert_eq!(
			skipped(&case),
			SkipReason::MinReleaseAgeNotMet {
				eligible_at: NOW - 1_200 + 3_600
			}
		);

		// Exactly old enough.
		case.window_start = Some(NOW - 3_600);
		assert_eq!(case.decide(), Decision::Apply);

		// A per-app override of one day.
		case.policy.min_age_seconds = Some(86_400);
		assert!(matches!(
			skipped(&case),
			SkipReason::MinReleaseAgeNotMet { .. }
		));

		// A zero delay is a real choice.
		case.policy.min_age_seconds = Some(0);
		case.window_start = Some(NOW);
		assert_eq!(case.decide(), Decision::Apply);
	}

	#[test]
	fn an_app_override_beats_the_global_age_in_both_directions() {
		let policy = AppPolicy {
			min_age_seconds: Some(60),
			..AppPolicy::default()
		};

		assert_eq!(
			effective_age(&policy, MinimumAge::DEFAULT),
			(MinimumAge::from_seconds(60).unwrap(), true)
		);
		assert_eq!(
			effective_age(&AppPolicy::default(), MinimumAge::DEFAULT),
			(MinimumAge::DEFAULT, false)
		);
	}

	#[test]
	fn pins_and_pauses_hold_automatic_updates() {
		let mut case = Case::new();
		case.policy.auto_paused = true;
		assert_eq!(skipped(&case), SkipReason::AutoPaused);

		case.policy.auto_paused = false;
		case.policy.pinned_release_id = Some(1);
		assert_eq!(skipped(&case), SkipReason::Pinned);
	}

	#[test]
	fn an_operator_run_ignores_mode_pause_and_age_but_never_a_pin() {
		let mut case = Case::new();
		case.initiator = Initiator::Operator;
		case.mode = UpdateMode::Manual;
		case.policy.auto_paused = true;
		case.window_start = Some(NOW);

		assert_eq!(case.decide(), Decision::Apply);

		case.policy.pinned_release_id = Some(1);
		assert_eq!(skipped(&case), SkipReason::Pinned);
	}

	#[test]
	fn unreviewed_sources_need_their_own_opt_in() {
		let mut case = Case::new();
		case.curated = false;
		assert_eq!(skipped(&case), SkipReason::SourceNotEnrolled);

		case.policy.unattended_custom = true;
		assert_eq!(case.decide(), Decision::Apply);
	}

	#[test]
	fn a_rebuilt_artifact_waits_for_a_person_but_an_operator_may_proceed() {
		let mut case = Case::new();
		case.outlook = Outlook::Ready(Box::new(ready(true)));
		assert_eq!(skipped(&case), SkipReason::RebuiltArtifact);

		case.initiator = Initiator::Operator;
		assert_eq!(case.decide(), Decision::Apply);
	}

	#[test]
	fn an_uncertain_clock_blocks_automatic_updates_only() {
		let mut case = Case::new();
		case.clock_ok = false;
		assert_eq!(skipped(&case), SkipReason::ClockUncertain);

		case.initiator = Initiator::Operator;
		assert_eq!(case.decide(), Decision::Apply);
	}

	#[test]
	fn every_outlook_that_is_not_an_update_explains_itself() {
		let mut case = Case::new();

		case.outlook = Outlook::UpToDate;
		assert_eq!(skipped(&case), SkipReason::NotNewer);

		case.outlook = Outlook::Unavailable("no build".into());
		assert_eq!(skipped(&case), SkipReason::Unavailable("no build".into()));

		case.outlook = Outlook::Choose(Vec::new());
		assert_eq!(skipped(&case), SkipReason::Ambiguous);

		case.outlook = Outlook::Failed("offline".into());
		assert_eq!(skipped(&case), SkipReason::CheckFailed("offline".into()));
	}

	#[test]
	fn skip_codes_are_stable_and_distinct() {
		let reasons = [
			SkipReason::ModeManual,
			SkipReason::Pinned,
			SkipReason::AutoPaused,
			SkipReason::SourceNotEnrolled,
			SkipReason::NotNewer,
			SkipReason::RebuiltArtifact,
			SkipReason::MinReleaseAgeNotMet { eligible_at: 0 },
			SkipReason::ClockUncertain,
			SkipReason::Unavailable(String::new()),
			SkipReason::Ambiguous,
			SkipReason::CheckFailed(String::new()),
		];
		let codes: std::collections::HashSet<_> = reasons.iter().map(SkipReason::code).collect();

		assert_eq!(codes.len(), reasons.len());
		assert!(codes.contains("MIN_RELEASE_AGE_NOT_MET"));
	}
}
