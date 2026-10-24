//! Searching and filtering the verified catalog.
//!
//! The work runs on the blocking pool and fans out across cores with rayon,
//! so a 10,000-project catalog filters without touching the interface
//! thread. Results are small owned rows, cheap to hand back and render.

use std::sync::Arc;

use fuzzy_matcher::FuzzyMatcher;
use fuzzy_matcher::skim::SkimMatcherV2;
use launcher_catalog::{Maturity, Project, ProjectKind, State, VerifiedCatalog};
use launcher_model::{HostTarget, Os};
use rayon::prelude::*;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ExploreQuery {
	/// Whitespace-separated terms. Every term must match somewhere.
	pub text: String,
	/// Hide apps with no package for this device.
	pub compatible_only: bool,
}

/// How a result matched what was typed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Match {
	/// Every word appears in the app's name, address, summary, or categories.
	#[default]
	Exact,
	/// Nothing matched exactly, so this is the closest by spelling: letters in
	/// order but not together, such as `vscode` for Visual Studio Code or
	/// `frefox` for Firefox.
	Close,
}

/// Whether an app has a package for the machine the launcher runs on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Availability {
	/// A package exists for this device.
	ThisDevice,
	/// A package exists for this device, but needs a capability this build
	/// does not have yet.
	NeedsNewerLauncher,
	/// Packages exist, but only for other platforms.
	OtherPlatforms(Vec<Os>),
	/// The catalog lists the app but no managed package.
	NoPackages,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExploreRow {
	pub slug: String,
	pub name: String,
	pub summary: String,
	pub kind: ProjectKind,
	pub status: State,
	/// `owner/repository`, from the signed record.
	pub repository: String,
	pub categories: Vec<String>,
	pub availability: Availability,
	/// How finished the app is, if the catalog says.
	pub maturity: Option<Maturity>,
	/// Programming languages, as the catalog lists them.
	pub languages: Vec<String>,
	/// The systems it has a package for, in a steady order.
	pub platforms: Vec<Os>,
	/// Highlighted by the catalog's editors.
	pub featured: bool,
	/// When the app joined the catalog, as Unix seconds, if it says.
	pub added_at: Option<i64>,
	pub matched: Match,
}

/// Runs `query` against `catalog`.
pub async fn search(
	catalog: Arc<VerifiedCatalog>,
	query: ExploreQuery,
	supported_capabilities: &'static [&'static str],
) -> Vec<ExploreRow> {
	let work = move || {
		search_blocking(
			&catalog,
			&query,
			HostTarget::current(),
			supported_capabilities,
		)
	};

	tokio::task::spawn_blocking(work)
		.await
		.unwrap_or_else(|error| {
			tracing::error!(%error, "catalog search task failed");
			Vec::new()
		})
}

pub(crate) fn search_blocking(
	catalog: &VerifiedCatalog,
	query: &ExploreQuery,
	host: Option<HostTarget>,
	supported: &[&str],
) -> Vec<ExploreRow> {
	let terms: Vec<String> = query
		.text
		.split_whitespace()
		.map(str::to_lowercase)
		.collect();
	let projects: Vec<_> = catalog.projects().collect();
	let texts = catalog.search_texts();
	let usable =
		|row: &ExploreRow| !query.compatible_only || row.availability == Availability::ThisDevice;

	let mut rows: Vec<ExploreRow> = projects
		.par_iter()
		.zip(texts.par_iter())
		.filter(|(_, text)| terms.iter().all(|term| text.contains(term.as_str())))
		.map(|((slug, project), _)| row(slug, project, host, supported))
		.filter(&usable)
		.collect();

	rows.par_sort_by_cached_key(|row| row.name.to_lowercase());

	// Nothing matched as typed. Offer what is closest by spelling, rather
	// than an empty page, but never mixed in with exact results.
	if rows.is_empty() && !terms.is_empty() {
		rows = close_matches(&projects, texts, &terms, host, supported)
			.into_iter()
			.filter(&usable)
			.collect();
	}

	rows
}

/// A word has to be at least this long to be matched by spelling. Shorter
/// ones would match nearly anything.
const MIN_FUZZY_CHARS: usize = 3;

/// How well a word has to match, per character of it. A good match scores
/// around 16 a character; scattered ones fall well below this.
const MIN_SCORE_PER_CHAR: i64 = 12;

/// How far apart the matched letters may be, as a multiple of the word.
const MAX_SPREAD_PER_CHAR: usize = 4;

/// Apps whose name, slug, and repository contain each word's letters in order,
/// best first. The matching is the `fuzzy-matcher` crate's.
fn close_matches(
	projects: &[(&str, &Project)],
	texts: &[String],
	terms: &[String],
	host: Option<HostTarget>,
	supported: &[&str],
) -> Vec<ExploreRow> {
	let matcher = SkimMatcherV2::default().ignore_case();

	let mut scored: Vec<(i64, ExploreRow)> = projects
		.par_iter()
		.zip(texts.par_iter())
		.filter_map(|((slug, project), text)| {
			// What people type is a name, so match against names only. The
			// repository is included because it usually repeats the name.
			let names = format!(
				"{} {} {}",
				project.display.name, slug, project.source.repository
			);
			let mut total = 0;

			for term in terms {
				let chars = term.chars().count();

				total += if chars < MIN_FUZZY_CHARS {
					// Too short to guess at. It has to be there as typed.
					text.contains(term.as_str()).then_some(0)?
				} else {
					let (score, at) = matcher.fuzzy_indices(&names, term)?;
					let spread = at.last()? - at.first()?;
					let wide = spread > MAX_SPREAD_PER_CHAR * chars + 2;

					if wide || score < MIN_SCORE_PER_CHAR * i64::try_from(chars).ok()? {
						return None;
					}

					score
				};
			}

			let mut found = row(slug, project, host, supported);

			found.matched = Match::Close;

			Some((total, found))
		})
		.collect();

	scored.par_sort_by(|a, b| {
		b.0.cmp(&a.0)
			.then_with(|| a.1.name.to_lowercase().cmp(&b.1.name.to_lowercase()))
	});

	scored.into_iter().map(|(_, row)| row).collect()
}

fn row(slug: &str, project: &Project, host: Option<HostTarget>, supported: &[&str]) -> ExploreRow {
	ExploreRow {
		slug: slug.to_owned(),
		name: project.display.name.clone(),
		summary: project.display.summary.clone(),
		kind: project.display.kind,
		status: project.status.state,
		repository: project.source.display_name(),
		categories: project.display.categories.clone(),
		availability: availability(project, host, supported),
		maturity: project.display.maturity,
		languages: project.display.languages.clone(),
		platforms: platforms(project),
		featured: project.display.featured,
		added_at: project.display.added_at_unix(),
		matched: Match::Exact,
	}
}

/// The systems with at least one package, in the order of [`Os`].
fn platforms(project: &Project) -> Vec<Os> {
	const ORDER: [Os; 10] = [
		Os::Windows,
		Os::Macos,
		Os::Linux,
		Os::Freebsd,
		Os::Netbsd,
		Os::Openbsd,
		Os::Dragonfly,
		Os::Illumos,
		Os::Solaris,
		Os::Haiku,
	];

	ORDER
		.into_iter()
		.filter(|os| project.variants.iter().any(|variant| variant.os == *os))
		.collect()
}

pub(crate) fn availability(
	project: &Project,
	host: Option<HostTarget>,
	supported: &[&str],
) -> Availability {
	if project.variants.is_empty() {
		return Availability::NoPackages;
	}

	let for_this_device: Vec<_> = project
		.variants
		.iter()
		.filter(|variant| host.is_some_and(|host| host.runs(variant.os, variant.arch)))
		.collect();

	if for_this_device.is_empty() {
		let mut systems: Vec<Os> = project.variants.iter().map(|variant| variant.os).collect();
		systems.sort_by_key(|os| os.label());
		systems.dedup();

		return Availability::OtherPlatforms(systems);
	}

	let usable = for_this_device.iter().any(|variant| {
		variant
			.required_capabilities
			.iter()
			.chain(&variant.install.required_capabilities)
			.all(|needed| supported.contains(&needed.as_str()))
	});

	if usable {
		Availability::ThisDevice
	} else {
		Availability::NeedsNewerLauncher
	}
}

#[cfg(test)]
mod tests {
	use std::path::Path;

	use launcher_catalog::{FreshnessPolicy, KeyRegistry, VerifyContext, verify};
	use launcher_model::Arch;
	use time::OffsetDateTime;

	use super::*;

	fn sample() -> VerifiedCatalog {
		let root = Path::new(env!("CARGO_MANIFEST_DIR"))
			.join("../../domain/launcher-catalog/tests/fixtures");
		let registry = KeyRegistry::from_test_fixture(
			&std::fs::read_to_string(root.join("test-public-key-registry.json")).unwrap(),
		)
		.unwrap();
		let manifest = std::fs::read(root.join("valid/launcher-catalog.json")).unwrap();
		let envelope = std::fs::read(root.join("valid/launcher-catalog.sig")).unwrap();

		let context = VerifyContext {
			registry: &registry,
			feed_id: "test.remade.launcher-catalog",
			now: OffsetDateTime::from_unix_timestamp(1_894_665_600).unwrap(),
			high_water: None,
			supported_capabilities: &["safe-archive-v1"],
			freshness: FreshnessPolicy::RequireCurrent,
		};

		verify(&manifest, &envelope, &context).unwrap().catalog
	}

	const LINUX: HostTarget = HostTarget {
		os: Os::Linux,
		arch: Arch::X86_64,
	};
	const MAC: HostTarget = HostTarget {
		os: Os::Macos,
		arch: Arch::Aarch64,
	};

	fn query(text: &str, compatible_only: bool) -> ExploreQuery {
		ExploreQuery {
			text: text.into(),
			compatible_only,
		}
	}

	#[test]
	fn empty_query_lists_everything() {
		let rows = search_blocking(
			&sample(),
			&query("", false),
			Some(LINUX),
			&["safe-archive-v1"],
		);

		assert_eq!(rows.len(), 1);
		assert_eq!(rows[0].repository, "fixture-owner/fixture-editor");
	}

	#[test]
	fn every_term_must_match() {
		let catalog = sample();
		let find =
			|text: &str| search_blocking(&catalog, &query(text, false), Some(LINUX), &[]).len();

		assert_eq!(find("sample editor"), 1);
		assert_eq!(find("SAMPLE graphics"), 1);
		assert_eq!(find("sample banana"), 0);
		assert_eq!(find("fixture-owner"), 1);
	}

	#[test]
	fn availability_reflects_the_host_and_capabilities() {
		let catalog = sample();
		let availability = |host, supported: &[&str]| {
			search_blocking(&catalog, &query("", false), host, supported)[0]
				.availability
				.clone()
		};

		assert_eq!(
			availability(Some(LINUX), &["safe-archive-v1"]),
			Availability::ThisDevice
		);
		assert_eq!(
			availability(Some(LINUX), &[]),
			Availability::NeedsNewerLauncher
		);
		assert_eq!(
			availability(Some(MAC), &["safe-archive-v1"]),
			Availability::OtherPlatforms(vec![Os::Linux])
		);
		assert_eq!(
			availability(None, &[]),
			Availability::OtherPlatforms(vec![Os::Linux])
		);
	}

	#[test]
	fn compatible_only_hides_other_platforms() {
		let catalog = sample();

		assert_eq!(
			search_blocking(&catalog, &query("", true), Some(MAC), &[]).len(),
			0
		);
		assert_eq!(
			search_blocking(
				&catalog,
				&query("", true),
				Some(LINUX),
				&["safe-archive-v1"]
			)
			.len(),
			1
		);
	}

	fn found(text: &str) -> Vec<ExploreRow> {
		search_blocking(&sample(), &query(text, false), Some(LINUX), &[])
	}

	#[test]
	fn skipped_swapped_and_abbreviated_spellings_still_find_the_app() {
		for typed in ["samle", "smpl", "sampel", "sample edtr", "sample-edtr"] {
			let rows = found(typed);

			assert_eq!(rows.len(), 1, "{typed}");
			assert_eq!(rows[0].matched, Match::Close, "{typed}");
		}
	}

	#[test]
	fn an_exact_match_is_never_labelled_close_and_close_is_only_a_fallback() {
		let rows = found("sample");

		assert_eq!(rows.len(), 1);
		assert_eq!(rows[0].matched, Match::Exact);
		assert_eq!(found("graphics")[0].matched, Match::Exact);
	}

	#[test]
	fn words_with_nothing_like_them_find_nothing() {
		for typed in ["zzzz", "xqzv", "banana", "sample banana"] {
			assert!(found(typed).is_empty(), "{typed}");
		}
	}

	#[test]
	fn very_short_words_are_never_guessed_at() {
		// "sm" is in the name only as separated letters, and two letters would
		// match almost anything, so no guess is made.
		assert!(found("se").is_empty() || found("se")[0].matched == Match::Exact);
		assert!(found("sxe").is_empty());
	}

	#[test]
	fn letters_in_the_wrong_order_are_not_close() {
		assert!(found("rdts").is_empty());
		assert!(found("lpmas").is_empty());
	}
}
