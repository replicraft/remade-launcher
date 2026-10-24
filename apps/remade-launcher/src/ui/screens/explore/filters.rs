//! Sorting, filtering, and grouping catalog rows. Pure functions, so the
//! rules are easy to test.

use launcher_catalog::{Maturity, ProjectKind};
use launcher_core::{Availability, ExploreRow, Match};
use launcher_model::Os;

use crate::ui::i18n::text;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum Sort {
	#[default]
	NameAscending,
	NameDescending,
	Newest,
}

impl Sort {
	/// The stable name saved with the person's preferences.
	pub(super) const fn key(self) -> &'static str {
		match self {
			Self::NameAscending => "name",
			Self::NameDescending => "name_desc",
			Self::Newest => "newest",
		}
	}

	pub(super) fn from_key(key: &str) -> Option<Self> {
		Self::ALL.into_iter().find(|sort| sort.key() == key)
	}

	pub(super) const ALL: [Self; 3] = [Self::NameAscending, Self::NameDescending, Self::Newest];

	pub(super) fn label(self) -> String {
		match self {
			Self::NameAscending => t!("explore-sort-name-asc"),
			Self::NameDescending => t!("explore-sort-name-desc"),
			Self::Newest => t!("explore-sort-newest"),
		}
	}
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum KindFilter {
	#[default]
	All,
	Only(ProjectKind),
}

impl KindFilter {
	pub(super) const ALL: [Self; 5] = [
		Self::All,
		Self::Only(ProjectKind::Desktop),
		Self::Only(ProjectKind::Cli),
		Self::Only(ProjectKind::Service),
		Self::Only(ProjectKind::Launcher),
	];

	pub(super) fn label(self) -> String {
		match self {
			Self::All => t!("explore-kind-all"),
			Self::Only(ProjectKind::Desktop) => t!("explore-kind-desktop-plural"),
			Self::Only(ProjectKind::Cli) => t!("explore-kind-cli-plural"),
			Self::Only(ProjectKind::Service) => t!("explore-kind-service-plural"),
			Self::Only(ProjectKind::Launcher) => t!("explore-kind-launcher-plural"),
		}
	}

	fn allows(self, kind: ProjectKind) -> bool {
		match self {
			Self::All => true,
			Self::Only(wanted) => wanted == kind,
		}
	}
}

#[derive(Clone, Debug, Default)]
pub(super) struct Filter {
	pub(super) sort: Sort,
	pub(super) kind: KindFilter,
	pub(super) category: Option<String>,
	pub(super) language: Option<String>,
	pub(super) platform: Option<Os>,
}

impl Filter {
	/// Nothing narrows the list, so the page can show its sections.
	pub(super) fn is_open(&self, query: &str) -> bool {
		query.trim().is_empty()
			&& self.kind == KindFilter::All
			&& self.category.is_none()
			&& self.language.is_none()
			&& self.platform.is_none()
	}
}

/// The rows to show, filtered and sorted.
pub(super) fn visible<'a>(rows: &'a [ExploreRow], filter: &Filter) -> Vec<&'a ExploreRow> {
	let mut shown: Vec<&ExploreRow> = rows
		.iter()
		.filter(|row| filter.kind.allows(row.kind))
		.filter(|row| {
			filter
				.category
				.as_ref()
				.is_none_or(|wanted| row.categories.contains(wanted))
		})
		.filter(|row| {
			filter
				.language
				.as_ref()
				.is_none_or(|wanted| row.languages.contains(wanted))
		})
		.filter(|row| {
			filter
				.platform
				.is_none_or(|wanted| row.platforms.contains(&wanted))
		})
		.collect();

	// Close matches come best-first from the search. Re-sorting them by name
	// would bury the best one.
	if shown.iter().any(|row| row.matched == Match::Close) {
		return shown;
	}

	match filter.sort {
		Sort::NameAscending => shown.sort_by_key(|row| row.name.to_lowercase()),
		Sort::NameDescending => {
			shown.sort_by_key(|row| std::cmp::Reverse(row.name.to_lowercase()));
		}
		Sort::Newest => shown.sort_by_key(|row| std::cmp::Reverse(row.added_at)),
	}

	shown
}

/// Categories with how many rows use each, biggest first.
pub(super) fn categories(rows: &[ExploreRow]) -> Vec<(String, usize)> {
	let mut counts: std::collections::BTreeMap<&str, usize> = std::collections::BTreeMap::new();

	for category in rows.iter().flat_map(|row| &row.categories) {
		*counts.entry(category).or_default() += 1;
	}

	let mut counted: Vec<(String, usize)> = counts
		.into_iter()
		.map(|(name, count)| (name.to_owned(), count))
		.collect();

	counted.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));

	counted
}

/// Languages in use, most common first.
pub(super) fn languages(rows: &[ExploreRow]) -> Vec<String> {
	let mut counts: std::collections::BTreeMap<&str, usize> = std::collections::BTreeMap::new();

	for language in rows.iter().flat_map(|row| &row.languages) {
		*counts.entry(language).or_default() += 1;
	}

	let mut counted: Vec<(&str, usize)> = counts.into_iter().collect();

	counted.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(b.0)));

	counted
		.into_iter()
		.map(|(name, _)| name.to_owned())
		.collect()
}

/// Systems in use, in the order first seen.
pub(super) fn platforms(rows: &[ExploreRow]) -> Vec<Os> {
	let mut found: Vec<Os> = Vec::new();

	for os in rows.iter().flat_map(|row| &row.platforms) {
		if !found.contains(os) {
			found.push(*os);
		}
	}

	found
}

pub(super) fn featured(rows: &[ExploreRow]) -> Vec<&ExploreRow> {
	let mut picked: Vec<&ExploreRow> = rows.iter().filter(|row| row.featured).collect();

	picked.sort_by_key(|row| row.name.to_lowercase());

	picked
}

/// The newest additions that say when they were added, newest first.
pub(super) fn recent(rows: &[ExploreRow], limit: usize) -> Vec<&ExploreRow> {
	let mut dated: Vec<&ExploreRow> = rows.iter().filter(|row| row.added_at.is_some()).collect();

	dated.sort_by_key(|row| std::cmp::Reverse(row.added_at));
	dated.truncate(limit);

	dated
}

/// A kind of app, as one app is described: "Desktop app".
pub(super) fn kind_name(kind: ProjectKind) -> String {
	match kind {
		ProjectKind::Desktop => t!("explore-kind-desktop"),
		ProjectKind::Cli => t!("explore-kind-cli"),
		ProjectKind::Service => t!("explore-kind-service"),
		ProjectKind::Launcher => t!("explore-kind-launcher"),
	}
}

/// How finished an app is.
pub(super) fn maturity_name(maturity: Maturity) -> String {
	match maturity {
		Maturity::Planned => t!("explore-maturity-planned"),
		Maturity::Experimental => t!("explore-maturity-experimental"),
		Maturity::Alpha => t!("explore-maturity-alpha"),
		Maturity::Beta => t!("explore-maturity-beta"),
		Maturity::Stable => t!("explore-maturity-stable"),
		Maturity::Archived => t!("explore-maturity-archived"),
	}
}

/// A category by its slug. Known ones are translated; any other reads as its
/// slug does: "developer-tools" as "Developer tools".
pub(super) fn category_label(slug: &str) -> String {
	let id = format!("explore-category-{slug}");
	let known = text(&id);

	if known != id {
		return known;
	}

	let spaced = slug.replace('-', " ");
	let mut characters = spaced.chars();

	match characters.next() {
		Some(first) => first.to_uppercase().chain(characters).collect(),
		None => String::new(),
	}
}

/// Rows matching the search text, the way the engine would filter them. Used
/// only for the demo catalog.
pub(super) fn filter_demo(
	rows: &[ExploreRow],
	query: &str,
	compatible_only: bool,
) -> Vec<ExploreRow> {
	let needle = query.trim().to_lowercase();

	rows.iter()
		.filter(|row| !compatible_only || row.availability == Availability::ThisDevice)
		.filter(|row| {
			needle.is_empty()
				|| row.name.to_lowercase().contains(&needle)
				|| row.summary.to_lowercase().contains(&needle)
				|| row.repository.to_lowercase().contains(&needle)
				|| row
					.categories
					.iter()
					.any(|category| category.contains(&needle))
		})
		.cloned()
		.collect()
}

#[cfg(test)]
mod tests {
	use launcher_catalog::State;

	use super::*;

	fn row(
		name: &str,
		kind: ProjectKind,
		categories: &[&str],
		featured: bool,
		added_at: Option<i64>,
	) -> ExploreRow {
		ExploreRow {
			slug: name.to_lowercase(),
			name: name.into(),
			summary: String::new(),
			kind,
			status: State::Active,
			repository: format!("o/{name}"),
			categories: categories.iter().map(|c| (*c).to_owned()).collect(),
			availability: Availability::ThisDevice,
			maturity: None,
			languages: Vec::new(),
			platforms: Vec::new(),
			featured,
			added_at,
			matched: Match::Exact,
		}
	}

	fn sample() -> Vec<ExploreRow> {
		vec![
			row("Beta", ProjectKind::Desktop, &["writing"], true, Some(10)),
			row(
				"alpha",
				ProjectKind::Cli,
				&["writing", "developer-tools"],
				false,
				Some(30),
			),
			row("Gamma", ProjectKind::Desktop, &["audio"], true, None),
		]
	}

	fn names(rows: &[&ExploreRow]) -> Vec<String> {
		rows.iter().map(|row| row.name.clone()).collect()
	}

	#[test]
	fn close_matches_keep_the_searchs_best_first_order() {
		let mut rows = sample();

		for row in &mut rows {
			row.matched = Match::Close;
		}

		let order =
			|rows: &[ExploreRow]| rows.iter().map(|row| row.name.clone()).collect::<Vec<_>>();
		let filter = Filter {
			sort: Sort::NameDescending,
			..Filter::default()
		};

		assert_eq!(
			visible(&rows, &filter)
				.iter()
				.map(|row| row.name.clone())
				.collect::<Vec<_>>(),
			order(&rows)
		);
	}

	#[test]
	fn names_sort_without_regard_to_case() {
		let rows = sample();

		assert_eq!(
			names(&visible(&rows, &Filter::default())),
			["alpha", "Beta", "Gamma"]
		);
		assert_eq!(
			names(&visible(
				&rows,
				&Filter {
					sort: Sort::NameDescending,
					..Filter::default()
				}
			)),
			["Gamma", "Beta", "alpha"]
		);
	}

	#[test]
	fn newest_puts_undated_apps_last() {
		let rows = sample();
		let filter = Filter {
			sort: Sort::Newest,
			..Filter::default()
		};

		assert_eq!(names(&visible(&rows, &filter)), ["alpha", "Beta", "Gamma"]);
	}

	#[test]
	fn kind_and_category_narrow_the_list() {
		let rows = sample();
		let desktop = Filter {
			kind: KindFilter::Only(ProjectKind::Desktop),
			..Filter::default()
		};
		let writing = Filter {
			category: Some("writing".into()),
			..Filter::default()
		};

		assert_eq!(names(&visible(&rows, &desktop)), ["Beta", "Gamma"]);
		assert_eq!(names(&visible(&rows, &writing)), ["alpha", "Beta"]);
	}

	#[test]
	fn categories_are_counted_biggest_first() {
		assert_eq!(
			categories(&sample()),
			[
				("writing".to_owned(), 2),
				("audio".to_owned(), 1),
				("developer-tools".to_owned(), 1)
			]
		);
	}

	#[test]
	fn featured_and_recent_pick_the_right_rows() {
		let rows = sample();

		assert_eq!(names(&featured(&rows)), ["Beta", "Gamma"]);
		assert_eq!(names(&recent(&rows, 1)), ["alpha"]);
		assert_eq!(names(&recent(&rows, 5)), ["alpha", "Beta"]);
	}

	#[test]
	fn the_page_shows_sections_only_when_nothing_narrows_it() {
		let open = Filter::default();

		assert!(open.is_open("  "));
		assert!(!open.is_open("ed"));
		assert!(
			!Filter {
				category: Some("audio".into()),
				..Filter::default()
			}
			.is_open("")
		);
	}

	#[test]
	fn language_and_platform_filters_narrow_the_list() {
		let mut rows = sample();

		rows[0].languages = vec!["Rust".into()];
		rows[0].platforms = vec![Os::Macos, Os::Linux];
		rows[1].languages = vec!["Rust".into(), "Go".into()];
		rows[1].platforms = vec![Os::Windows];

		let rust = Filter {
			language: Some("Rust".into()),
			..Filter::default()
		};
		let mac = Filter {
			platform: Some(Os::Macos),
			..Filter::default()
		};

		assert_eq!(names(&visible(&rows, &rust)), ["alpha", "Beta"]);
		assert_eq!(names(&visible(&rows, &mac)), ["Beta"]);
		assert_eq!(languages(&rows), ["Rust", "Go"]);
		assert_eq!(platforms(&rows), [Os::Macos, Os::Linux, Os::Windows]);
		assert!(!rust.is_open(""));
	}

	#[test]
	fn slugs_read_as_labels() {
		assert_eq!(category_label("developer-tools"), "Developer tools");
		assert_eq!(category_label("audio"), "Audio");
	}
}
