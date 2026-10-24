//! Sorting and filtering the installed apps. Pure functions.

use std::collections::HashMap;

use launcher_core::{InstalledApp, UpdateStatus};
use launcher_model::InstallationId;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum Sort {
	#[default]
	Name,
	RecentlyInstalled,
	UpdatesFirst,
}

impl Sort {
	pub(super) const OPTIONS: [(Self, &'static str); 3] = [
		(Self::Name, "Name"),
		(Self::RecentlyInstalled, "Recently installed"),
		(Self::UpdatesFirst, "Updates first"),
	];
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum Show {
	#[default]
	All,
	UpdatesAvailable,
	Reviewed,
	AddedByYou,
}

impl Show {
	pub(super) const OPTIONS: [(Self, &'static str); 4] = [
		(Self::All, "All apps"),
		(Self::UpdatesAvailable, "Updates available"),
		(Self::Reviewed, "Reviewed by Remade"),
		(Self::AddedByYou, "Added by you"),
	];
}

/// What the Library's controls hold between frames.
#[derive(Default)]
pub(crate) struct LibraryState {
	pub(crate) search: String,
	pub(super) sort: Sort,
	pub(super) show: Show,
}

impl LibraryState {
	/// Puts the newest installs first, with no search or filter hiding them.
	/// Used when the person comes to the Library to find what just finished.
	pub(crate) fn show_recent(&mut self) {
		self.sort = Sort::RecentlyInstalled;
		self.show = Show::All;
		self.search.clear();
	}
}

fn has_update(statuses: &HashMap<InstallationId, UpdateStatus>, app: &InstalledApp) -> bool {
	statuses
		.get(&app.id)
		.is_some_and(|status| status.available.is_some())
}

/// The apps to show, matching the search, filter, and sort.
pub(super) fn arrange<'a>(
	apps: &'a [InstalledApp],
	statuses: &HashMap<InstallationId, UpdateStatus>,
	state: &LibraryState,
) -> Vec<&'a InstalledApp> {
	let needle = state.search.trim().to_lowercase();

	let mut shown: Vec<&InstalledApp> = apps
		.iter()
		.filter(|app| {
			needle.is_empty()
				|| app.name.to_lowercase().contains(&needle)
				|| app.repository.to_lowercase().contains(&needle)
		})
		.filter(|app| match state.show {
			Show::All => true,
			Show::UpdatesAvailable => has_update(statuses, app),
			Show::Reviewed => app.curated,
			Show::AddedByYou => !app.curated,
		})
		.collect();

	match state.sort {
		Sort::Name => shown.sort_by_key(|app| app.name.to_lowercase()),
		Sort::RecentlyInstalled => shown.sort_by_key(|app| std::cmp::Reverse(app.installed_at)),
		Sort::UpdatesFirst => {
			shown.sort_by_key(|app| (!has_update(statuses, app), app.name.to_lowercase()))
		}
	}

	shown
}

#[cfg(test)]
mod tests {
	use launcher_core::{AutoStatus, AvailableUpdate, Decision, SkipReason, UpdateState};
	use launcher_model::{MinimumAge, UpdateMode};

	use super::*;

	fn app(name: &str, curated: bool, installed_at: i64) -> InstalledApp {
		InstalledApp {
			id: InstallationId::new(),
			name: name.into(),
			repository: format!("o/{name}"),
			version: "v1".into(),
			curated,
			catalog_slug: None,
			root: String::new(),
			launchable: true,
			evidence: String::new(),
			installed_at,
		}
	}

	fn with_update(app: &InstalledApp) -> UpdateStatus {
		UpdateStatus {
			installation: app.id,
			name: app.name.clone(),
			catalog_slug: None,
			installed_version: "v1".into(),
			state: UpdateState::Available,
			available: Some(AvailableUpdate {
				version: "v2".into(),
				file: "a".into(),
				size: 1,
				published_at: None,
				rebuilt: false,
			}),
			auto: AutoStatus {
				mode: UpdateMode::Manual,
				minimum_age: MinimumAge::DEFAULT,
				age_is_per_app: false,
				eligible_at: None,
				decision: Decision::Skip(SkipReason::ModeManual),
			},
		}
	}

	fn names(apps: &[&InstalledApp]) -> Vec<String> {
		apps.iter().map(|app| app.name.clone()).collect()
	}

	#[test]
	fn search_matches_name_or_repository_ignoring_case() {
		let apps = [app("Quill", true, 1), app("hex-peek", false, 2)];
		let state = LibraryState {
			search: "PEEK".into(),
			..LibraryState::default()
		};

		assert_eq!(
			names(&arrange(&apps, &HashMap::new(), &state)),
			["hex-peek"]
		);
	}

	#[test]
	fn the_filter_separates_reviewed_apps_from_ones_the_person_added() {
		let apps = [app("Quill", true, 1), app("hex-peek", false, 2)];
		let reviewed = LibraryState {
			show: Show::Reviewed,
			..LibraryState::default()
		};
		let added = LibraryState {
			show: Show::AddedByYou,
			..LibraryState::default()
		};

		assert_eq!(
			names(&arrange(&apps, &HashMap::new(), &reviewed)),
			["Quill"]
		);
		assert_eq!(
			names(&arrange(&apps, &HashMap::new(), &added)),
			["hex-peek"]
		);
	}

	#[test]
	fn updates_can_be_filtered_and_sorted_to_the_top() {
		let apps = [app("Alpha", true, 1), app("Beta", true, 2)];
		let statuses = HashMap::from([(apps[1].id, with_update(&apps[1]))]);

		let only = LibraryState {
			show: Show::UpdatesAvailable,
			..LibraryState::default()
		};
		let first = LibraryState {
			sort: Sort::UpdatesFirst,
			..LibraryState::default()
		};

		assert_eq!(names(&arrange(&apps, &statuses, &only)), ["Beta"]);
		assert_eq!(names(&arrange(&apps, &statuses, &first)), ["Beta", "Alpha"]);
	}

	#[test]
	fn showing_recent_installs_clears_whatever_could_hide_them() {
		let mut state = LibraryState {
			search: "x".into(),
			sort: Sort::Name,
			show: Show::Reviewed,
		};

		state.show_recent();

		assert_eq!(state.sort, Sort::RecentlyInstalled);
		assert_eq!(state.show, Show::All);
		assert!(state.search.is_empty());
	}

	#[test]
	fn recently_installed_puts_the_newest_first() {
		let apps = [app("Alpha", true, 1), app("Beta", true, 2)];
		let state = LibraryState {
			sort: Sort::RecentlyInstalled,
			..LibraryState::default()
		};

		assert_eq!(
			names(&arrange(&apps, &HashMap::new(), &state)),
			["Beta", "Alpha"]
		);
	}
}
