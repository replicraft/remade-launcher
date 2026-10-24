//! Sorting and filtering the installed apps. Pure functions.

use std::collections::HashMap;

use launcher_core::{InstalledApp, UpdateStatus, close_score};
use launcher_model::InstallationId;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum Sort {
	#[default]
	Name,
	RecentlyInstalled,
	UpdatesFirst,
}

impl Sort {
	/// The stable name saved with the person's preferences.
	pub(super) const fn key(self) -> &'static str {
		match self {
			Self::Name => "name",
			Self::RecentlyInstalled => "recent",
			Self::UpdatesFirst => "updates",
		}
	}

	pub(super) fn from_key(key: &str) -> Option<Self> {
		Self::ALL.into_iter().find(|sort| sort.key() == key)
	}

	pub(super) const ALL: [Self; 3] = [Self::Name, Self::RecentlyInstalled, Self::UpdatesFirst];

	pub(super) fn label(self) -> String {
		match self {
			Self::Name => t!("library-sort-name"),
			Self::RecentlyInstalled => t!("library-sort-recent"),
			Self::UpdatesFirst => t!("library-sort-updates"),
		}
	}

	/// Every choice with its words, for the dropdown.
	pub(super) fn options() -> Vec<(Self, String)> {
		Self::ALL
			.into_iter()
			.map(|sort| (sort, sort.label()))
			.collect()
	}
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
	pub(super) fn label(self) -> String {
		match self {
			Self::All => t!("library-show-all"),
			Self::UpdatesAvailable => t!("library-show-updates"),
			Self::Reviewed => t!("library-show-reviewed"),
			Self::AddedByYou => t!("library-show-added"),
		}
	}

	/// Every choice with its words, for the dropdown.
	pub(super) fn options() -> Vec<(Self, String)> {
		[
			Self::All,
			Self::UpdatesAvailable,
			Self::Reviewed,
			Self::AddedByYou,
		]
		.into_iter()
		.map(|show| (show, show.label()))
		.collect()
	}
}

/// What the Library's controls hold between frames.
#[derive(Default)]
pub(crate) struct LibraryState {
	pub(crate) search: String,
	pub(super) sort: Sort,
	pub(super) show: Show,
}

impl LibraryState {
	pub(crate) fn sort_key(&self) -> &'static str {
		self.sort.key()
	}

	/// Restores a saved sort. An unknown name is ignored.
	pub(crate) fn set_sort_key(&mut self, key: &str) {
		if let Some(sort) = Sort::from_key(key) {
			self.sort = sort;
		}
	}

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
	let exact = |app: &InstalledApp| {
		needle.is_empty()
			|| app.name.to_lowercase().contains(&needle)
			|| app.repository.to_lowercase().contains(&needle)
	};

	// Nothing contains what was typed, so fall back to the closest by
	// spelling, the way the catalog search does.
	let by_spelling = !apps.iter().any(exact);
	let terms: Vec<String> = needle.split_whitespace().map(str::to_owned).collect();
	let close = |app: &InstalledApp| {
		let text = format!("{} {}", app.name, app.repository);

		close_score(&text, &text, &terms).is_some()
	};

	let mut shown: Vec<&InstalledApp> = apps
		.iter()
		.filter(|app| if by_spelling { close(app) } else { exact(app) })
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
	fn a_search_that_matches_nothing_falls_back_to_the_closest_spelling() {
		let apps = vec![app("Firefox", true, 1), app("Inkscape", true, 2)];
		let search = |text: &str| {
			let state = LibraryState {
				search: text.into(),
				..LibraryState::default()
			};

			arrange(&apps, &HashMap::new(), &state)
				.iter()
				.map(|app| app.name.clone())
				.collect::<Vec<_>>()
		};

		assert_eq!(search("fire"), ["Firefox"], "an exact match stays exact");
		assert_eq!(search("frefox"), ["Firefox"]);
		assert_eq!(search("inksc"), ["Inkscape"], "this one is a prefix");
		assert!(search("banana").is_empty());
		// Close matches are only a fallback: when something contains the text,
		// nothing else comes along.
		assert_eq!(search("ink"), ["Inkscape"]);
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
