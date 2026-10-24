//! Searching and filtering the verified catalog.
//!
//! The work runs on the blocking pool and fans out across cores with rayon,
//! so a 10,000-project catalog filters without touching the interface
//! thread. Results are small owned rows, cheap to hand back and render.

use std::sync::Arc;

use launcher_catalog::{Project, ProjectKind, State, VerifiedCatalog};
use launcher_model::{HostTarget, Os};
use rayon::prelude::*;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ExploreQuery {
	/// Whitespace-separated terms. Every term must match somewhere.
	pub text: String,
	/// Hide apps with no package for this device.
	pub compatible_only: bool,
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
	/// Highlighted by the catalog's editors.
	pub featured: bool,
	/// When the app joined the catalog, as Unix seconds, if it says.
	pub added_at: Option<i64>,
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

	let mut rows: Vec<ExploreRow> = projects
		.par_iter()
		.filter(|(slug, project)| matches(slug, project, &terms))
		.map(|(slug, project)| row(slug, project, host, supported))
		.filter(|row| !query.compatible_only || row.availability == Availability::ThisDevice)
		.collect();

	rows.par_sort_by_cached_key(|row| row.name.to_lowercase());

	rows
}

fn matches(slug: &str, project: &Project, terms: &[String]) -> bool {
	if terms.is_empty() {
		return true;
	}

	let display = &project.display;
	let haystack = [
		slug,
		&display.name,
		&display.summary,
		&project.source.owner,
		&project.source.repository,
	]
	.iter()
	.chain(
		&display
			.categories
			.iter()
			.map(String::as_str)
			.collect::<Vec<_>>(),
	)
	.map(|part| part.to_lowercase())
	.collect::<Vec<_>>()
	.join("\n");

	terms.iter().all(|term| haystack.contains(term.as_str()))
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
		featured: project.display.featured,
		added_at: project.display.added_at_unix(),
	}
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
}
