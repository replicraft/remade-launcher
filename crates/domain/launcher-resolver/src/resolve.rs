//! Resolution of a curated project against provider observations.
//!
//! The output is a [`Resolution`]: one candidate, a set the user must choose
//! from, or a precise reason nothing fits. Provider order is used as evidence
//! for opaque versions only, and it is labeled when it is.

use std::cmp::Ordering;

use launcher_catalog::{Channel, Project, State, Tracking, Variant};
use launcher_model::HostTarget;
use launcher_providers::{Asset, Release, ReleaseObservation};
use serde::Serialize;

use crate::matching::Matcher;
use crate::version::{Version, VersionOrder};

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ReleaseRef {
	pub id: u64,
	pub tag: String,
	pub page_url: String,
	pub prerelease: bool,
	pub order: OrderBasis,
}

/// A serializable stand-in for [`VersionOrder`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum OrderBasis {
	Defined,
	ProviderOrder,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct AssetRef {
	pub id: u64,
	pub name: String,
	pub size: u64,
	pub download_url: String,
	/// The host's claimed SHA-256. Not publisher evidence.
	pub host_sha256: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct SkippedRelease {
	pub tag: String,
	pub reason: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Candidate {
	pub release: ReleaseRef,
	pub variant_id: String,
	pub asset: AssetRef,
	/// Why this release, variant, and asset were chosen, in order.
	pub explanation: Vec<String>,
	/// Newer releases that were passed over, so the user can see why a newer
	/// version is not offered.
	pub skipped_newer: Vec<SkippedRelease>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NoCompatible {
	/// The catalog entry has no variant for this device.
	NoVariantForDevice,
	/// A variant exists but needs a capability this build lacks.
	NeedsNewerLauncher,
	/// No release passed the channel, tag, and version rules.
	NoEligibleRelease { checked: usize },
	/// Eligible releases exist, but none has a package matching the rules.
	NoMatchingAsset { checked: usize },
	/// A pattern in the signed policy uses unsupported syntax.
	InvalidRule,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
	Candidate(Box<Candidate>),
	/// Several packages fit equally well. The user chooses; nothing is
	/// picked by API order.
	Ambiguous(Vec<Candidate>),
	NoCompatible(NoCompatible),
	/// The catalog entry is paused or revoked.
	Blocked(State),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Resolution {
	pub outcome: Outcome,
	/// False when the provider's paging stopped early. "Nothing compatible"
	/// then covers only the releases that were checked.
	pub discovery_complete: bool,
}

pub fn resolve_curated(
	project: &Project,
	observation: &ReleaseObservation,
	host: HostTarget,
	supported_capabilities: &[&str],
) -> Resolution {
	resolve_curated_on(project, observation, host, supported_capabilities, None)
}

/// Like [`resolve_curated`], with the user's own channel choice for this app.
///
/// A choice can only widen what a signed `stable` policy allows by opting in
/// to prereleases. Nothing here can select a release the tag rules, pin, or
/// platform rules exclude.
pub fn resolve_curated_on(
	project: &Project,
	observation: &ReleaseObservation,
	host: HostTarget,
	supported_capabilities: &[&str],
	channel: Option<Channel>,
) -> Resolution {
	let outcome = resolve_outcome(project, observation, host, supported_capabilities, channel);

	Resolution {
		outcome,
		discovery_complete: observation.complete,
	}
}

fn resolve_outcome(
	project: &Project,
	observation: &ReleaseObservation,
	host: HostTarget,
	supported: &[&str],
	channel: Option<Channel>,
) -> Outcome {
	if project.status.state != State::Active {
		return Outcome::Blocked(project.status.state);
	}

	let device_variants: Vec<&Variant> = project
		.variants
		.iter()
		.filter(|variant| host.runs(variant.os, variant.arch))
		.collect();

	if device_variants.is_empty() {
		return Outcome::NoCompatible(NoCompatible::NoVariantForDevice);
	}

	let usable: Vec<&Variant> = device_variants
		.into_iter()
		.filter(|variant| capabilities_met(variant, supported))
		.collect();

	if usable.is_empty() {
		return Outcome::NoCompatible(NoCompatible::NeedsNewerLauncher);
	}

	let Some(rules) = TagRules::new(project) else {
		return Outcome::NoCompatible(NoCompatible::InvalidRule);
	};

	let eligible = eligible_releases(project, observation, &rules, channel);

	if eligible.is_empty() {
		return Outcome::NoCompatible(NoCompatible::NoEligibleRelease {
			checked: observation.releases.len(),
		});
	}

	let mut skipped_newer = Vec::new();

	for entry in &eligible {
		match matches_in_release(entry.release, &usable, host) {
			Err(()) => return Outcome::NoCompatible(NoCompatible::InvalidRule),
			Ok(found) if found.is_empty() => skipped_newer.push(SkippedRelease {
				tag: entry.release.tag.clone(),
				reason: "It has no package for this device.".into(),
			}),
			Ok(found) => {
				let candidates: Vec<Candidate> = found
					.into_iter()
					.map(|(variant, asset)| candidate(entry, variant, asset, host, &skipped_newer))
					.collect();

				return match candidates.len() {
					1 => Outcome::Candidate(Box::new(
						candidates.into_iter().next().expect("one candidate"),
					)),
					_ => Outcome::Ambiguous(candidates),
				};
			}
		}
	}

	Outcome::NoCompatible(NoCompatible::NoMatchingAsset {
		checked: observation.releases.len(),
	})
}

fn capabilities_met(variant: &Variant, supported: &[&str]) -> bool {
	variant
		.required_capabilities
		.iter()
		.chain(&variant.install.required_capabilities)
		.all(|needed| supported.contains(&needed.as_str()))
}

struct TagRules {
	/// Each include pattern with its literal prefix, for version parsing.
	include: Vec<(String, Matcher)>,
	exclude: Matcher,
	product: Option<Matcher>,
}

impl TagRules {
	fn new(project: &Project) -> Option<Self> {
		let policy = &project.release_policy;

		let include = policy
			.tag_include
			.iter()
			.map(|pattern| {
				Some((
					literal_prefix(pattern),
					Matcher::new(std::slice::from_ref(pattern), true)?,
				))
			})
			.collect::<Option<Vec<_>>>()?;

		Some(Self {
			include,
			exclude: Matcher::new(&policy.tag_exclude, true)?,
			// The product filter scopes a monorepo's tags to one product.
			product: match &project.source.product_filter {
				Some(filter) => Some(Matcher::new(std::slice::from_ref(filter), true)?),
				None => None,
			},
		})
	}

	/// The literal prefix of the first include pattern that selects `tag`,
	/// or `None` if the tag is not allowed at all.
	fn allowed_prefix(&self, tag: &str) -> Option<&str> {
		let prefix = self
			.include
			.iter()
			.find(|(_, matcher)| matcher.is_match(tag))
			.map(|(prefix, _)| prefix.as_str())?;

		let allowed = !self.exclude.is_match(tag)
			&& self
				.product
				.as_ref()
				.is_none_or(|product| product.is_match(tag));

		allowed.then_some(prefix)
	}
}

/// The text before the first wildcard: `editor-v` for `editor-v*`.
fn literal_prefix(pattern: &str) -> String {
	pattern
		.chars()
		.take_while(|c| !matches!(c, '*' | '?' | '['))
		.collect()
}

struct Eligible<'a> {
	release: &'a Release,
	version: Version,
}

/// Releases that pass the channel, pin, tag, and version rules, newest first.
fn eligible_releases<'a>(
	project: &Project,
	observation: &'a ReleaseObservation,
	rules: &TagRules,
	channel: Option<Channel>,
) -> Vec<Eligible<'a>> {
	let policy = &project.release_policy;
	let channel = channel.unwrap_or(policy.channel);

	let mut eligible: Vec<Eligible<'a>> = observation
		.releases
		.iter()
		.filter(|release| match policy.tracking {
			Tracking::Pinned => {
				policy.pinned_release_id.as_deref() == Some(release.id.to_string().as_str())
			}
			Tracking::Upstream => true,
		})
		.filter_map(|release| {
			let prefix = rules.allowed_prefix(&release.tag)?;
			let version =
				Version::parse_with_prefix(&release.tag, prefix, policy.version_strategy)?;

			Some(Eligible { release, version })
		})
		.filter(|entry| {
			policy.channel != Channel::Stable
				|| (!entry.release.prerelease && !entry.version.is_prerelease())
		})
		.collect();

	// Newest first. Where the strategy defines no order, publication time is
	// the evidence, and ties keep the provider's order.
	eligible.sort_by(|a, b| {
		b.version
			.compare(&a.version)
			.unwrap_or(Ordering::Equal)
			.then_with(|| b.release.published_at.cmp(&a.release.published_at))
	});

	eligible
}

type Match<'a> = (&'a Variant, &'a Asset);

/// Variant/asset pairs in `release`, restricted to the best architecture.
fn matches_in_release<'a>(
	release: &'a Release,
	variants: &[&'a Variant],
	host: HostTarget,
) -> Result<Vec<Match<'a>>, ()> {
	let mut found: Vec<Match<'a>> = Vec::new();

	for variant in variants {
		let include =
			Matcher::new(&variant.assets.include, variant.assets.case_sensitive).ok_or(())?;
		let exclude =
			Matcher::new(&variant.assets.exclude, variant.assets.case_sensitive).ok_or(())?;

		for asset in &release.assets {
			if include.is_match(&asset.name) && !exclude.is_match(&asset.name) {
				found.push((variant, asset));
			}
		}
	}

	// Native beats universal. Only drop the universal ones if a native one
	// exists, so a universal-only release still resolves.
	if found.iter().any(|(variant, _)| variant.arch == host.arch) {
		found.retain(|(variant, _)| variant.arch == host.arch);
	}

	Ok(found)
}

fn candidate(
	entry: &Eligible<'_>,
	variant: &Variant,
	asset: &Asset,
	host: HostTarget,
	skipped_newer: &[SkippedRelease],
) -> Candidate {
	let release = entry.release;
	let order = match entry.version.order_kind() {
		VersionOrder::Defined => OrderBasis::Defined,
		VersionOrder::ProviderOrder => OrderBasis::ProviderOrder,
	};

	let mut explanation = vec![
		format!(
			"Release {} passed the channel, tag, and version rules.",
			release.tag
		),
		format!(
			"Package type {} ({}) is built for {} {}, which is this device.",
			variant.id,
			variant.format.label(),
			host.os.label(),
			variant.arch.label()
		),
		format!("File {} matched the package's filename rules.", asset.name),
	];

	if order == OrderBasis::ProviderOrder {
		explanation.push(
			"This project's version labels have no defined order, so the most recently published release was used."
				.into(),
		);
	}

	Candidate {
		release: ReleaseRef {
			id: release.id,
			tag: release.tag.clone(),
			page_url: release.page_url.clone(),
			prerelease: release.prerelease,
			order,
		},
		variant_id: variant.id.clone(),
		asset: AssetRef {
			id: asset.id,
			name: asset.name.clone(),
			size: asset.size,
			download_url: asset.download_url.clone(),
			host_sha256: asset.host_sha256.clone(),
		},
		explanation,
		skipped_newer: skipped_newer.to_vec(),
	}
}

#[cfg(test)]
mod tests {
	use launcher_model::{Arch, Os};
	use launcher_providers::RepositoryIdentity;
	use serde_json::json;

	use super::*;

	const LINUX: HostTarget = HostTarget {
		os: Os::Linux,
		arch: Arch::X86_64,
	};
	const MAC: HostTarget = HostTarget {
		os: Os::Macos,
		arch: Arch::Aarch64,
	};
	const CAPS: &[&str] = &["safe-archive-v1"];

	fn variant(id: &str, os: &str, arch: &str, include: &str) -> serde_json::Value {
		json!({
			"id": id, "os": os, "arch": arch, "abi": "native", "runtimes": [], "format": "tar.gz",
			"assets": {"include": [include], "exclude": ["*debug*"], "case_sensitive": true},
			"install": {"adapter": "portable-v1", "scope": "user", "entry_point": "bin/app",
				"required_capabilities": ["safe-archive-v1"]},
			"verification": {"minimum": "host-digest"},
			"required_capabilities": []
		})
	}

	fn project_with(variants: Vec<serde_json::Value>, policy: serde_json::Value) -> Project {
		let mut release_policy = json!({
			"channel": "stable", "version_strategy": "semver",
			"tag_include": ["v*"], "tag_exclude": [], "tracking": "upstream"
		});

		for (key, value) in policy.as_object().unwrap() {
			release_policy[key] = value.clone();
		}

		serde_json::from_value(json!({
			"display": {"name": "App", "summary": "s", "kind": "desktop", "categories": [], "links": {}},
			"source": {"provider": "github", "repository_id": "1", "owner_id": "2", "owner": "o", "repository": "r"},
			"status": {"state": "active"},
			"release_policy": release_policy,
			"variants": variants
		}))
		.unwrap()
	}

	fn linux_project() -> Project {
		project_with(
			vec![variant(
				"linux",
				"linux",
				"x86_64",
				"app-*-linux-x64.tar.gz",
			)],
			json!({}),
		)
	}

	fn asset(id: u64, name: &str) -> Asset {
		Asset {
			id,
			name: name.into(),
			size: 1,
			content_type: String::new(),
			host_sha256: None,
			created_at: None,
			updated_at: None,
			download_url: format!("https://github.com/o/r/releases/download/x/{name}"),
		}
	}

	fn release(id: u64, tag: &str, prerelease: bool, names: &[&str]) -> Release {
		Release {
			id,
			tag: tag.into(),
			name: None,
			prerelease,
			published_at: Some(format!("2030-01-{id:02}T00:00:00Z")),
			published_at_unix: None,
			notes: None,
			page_url: format!("https://github.com/o/r/releases/tag/{tag}"),
			assets: names
				.iter()
				.enumerate()
				.map(|(i, n)| asset(id * 100 + i as u64, n))
				.collect(),
		}
	}

	fn observed(releases: Vec<Release>, complete: bool) -> ReleaseObservation {
		ReleaseObservation {
			repository: RepositoryIdentity {
				repository_id: 1,
				owner_id: 2,
				owner: "o".into(),
				name: "r".into(),
			},
			releases,
			complete,
		}
	}

	fn chosen(resolution: Resolution) -> Candidate {
		match resolution.outcome {
			Outcome::Candidate(candidate) => *candidate,
			other => panic!("expected one candidate, got {other:?}"),
		}
	}

	#[test]
	fn picks_the_newest_compatible_release_by_version_not_list_order() {
		let observation = observed(
			vec![
				release(1, "v1.9.0", false, &["app-1.9.0-linux-x64.tar.gz"]),
				release(2, "v1.10.0", false, &["app-1.10.0-linux-x64.tar.gz"]),
			],
			true,
		);

		let candidate = chosen(resolve_curated(&linux_project(), &observation, LINUX, CAPS));

		assert_eq!(candidate.release.tag, "v1.10.0");
		assert_eq!(candidate.asset.name, "app-1.10.0-linux-x64.tar.gz");
		assert!(!candidate.explanation.is_empty());
	}

	#[test]
	fn explains_why_a_newer_release_was_passed_over() {
		let observation = observed(
			vec![
				release(3, "v2.0.0", false, &["app-2.0.0-windows.zip"]),
				release(2, "v1.0.0", false, &["app-1.0.0-linux-x64.tar.gz"]),
			],
			true,
		);

		let candidate = chosen(resolve_curated(&linux_project(), &observation, LINUX, CAPS));

		assert_eq!(candidate.release.tag, "v1.0.0");
		assert_eq!(candidate.skipped_newer.len(), 1);
		assert_eq!(candidate.skipped_newer[0].tag, "v2.0.0");
	}

	#[test]
	fn stable_ignores_prereleases_and_drafts_of_either_kind() {
		let observation = observed(
			vec![
				release(3, "v3.0.0", true, &["app-3.0.0-linux-x64.tar.gz"]),
				release(
					2,
					"v2.0.0-rc.1",
					false,
					&["app-2.0.0-rc.1-linux-x64.tar.gz"],
				),
				release(1, "v1.0.0", false, &["app-1.0.0-linux-x64.tar.gz"]),
			],
			true,
		);

		assert_eq!(
			chosen(resolve_curated(&linux_project(), &observation, LINUX, CAPS))
				.release
				.tag,
			"v1.0.0"
		);
	}

	#[test]
	fn a_users_prerelease_opt_in_widens_a_stable_policy_and_nothing_else() {
		let observation = observed(
			vec![
				release(2, "v2.0.0-rc.1", true, &["app-2.0.0-rc.1-linux-x64.tar.gz"]),
				release(1, "v1.0.0", false, &["app-1.0.0-linux-x64.tar.gz"]),
			],
			true,
		);
		let on = |channel| {
			chosen(resolve_curated_on(
				&linux_project(),
				&observation,
				LINUX,
				CAPS,
				channel,
			))
			.release
			.tag
		};

		assert_eq!(on(None), "v1.0.0");
		assert_eq!(on(Some(Channel::Stable)), "v1.0.0");
		assert_eq!(on(Some(Channel::Beta)), "v2.0.0-rc.1");

		// The tag rules still apply to a prerelease.
		let mut project = linux_project();
		project.release_policy.tag_exclude = vec!["*-rc.*".into()];

		assert_eq!(
			chosen(resolve_curated_on(
				&project,
				&observation,
				LINUX,
				CAPS,
				Some(Channel::Beta)
			))
			.release
			.tag,
			"v1.0.0"
		);
	}

	#[test]
	fn beta_channel_accepts_prereleases() {
		let project = project_with(
			vec![variant(
				"linux",
				"linux",
				"x86_64",
				"app-*-linux-x64.tar.gz",
			)],
			json!({"channel": "beta"}),
		);
		let observation = observed(
			vec![
				release(2, "v2.0.0-rc.1", true, &["app-2.0.0-rc.1-linux-x64.tar.gz"]),
				release(1, "v1.0.0", false, &["app-1.0.0-linux-x64.tar.gz"]),
			],
			true,
		);

		assert_eq!(
			chosen(resolve_curated(&project, &observation, LINUX, CAPS))
				.release
				.tag,
			"v2.0.0-rc.1"
		);
	}

	#[test]
	fn tag_filters_and_product_scope_apply_before_platform_ranking() {
		let mut project = project_with(
			vec![variant("linux", "linux", "x86_64", "*-linux-x64.tar.gz")],
			json!({"tag_include": ["editor-v*", "cli-v*"], "tag_exclude": ["cli-*"]}),
		);
		project.source.product_filter = Some("editor-*".into());

		let observation = observed(
			vec![
				release(3, "cli-v9.0.0", false, &["cli-linux-x64.tar.gz"]),
				release(2, "editor-v1.0.0", false, &["editor-linux-x64.tar.gz"]),
			],
			true,
		);

		assert_eq!(
			chosen(resolve_curated(&project, &observation, LINUX, CAPS))
				.release
				.tag,
			"editor-v1.0.0"
		);
	}

	#[test]
	fn a_pinned_project_resolves_only_its_release() {
		let project = project_with(
			vec![variant(
				"linux",
				"linux",
				"x86_64",
				"app-*-linux-x64.tar.gz",
			)],
			json!({"tracking": "pinned", "pinned_release_id": "1"}),
		);
		let observation = observed(
			vec![
				release(2, "v2.0.0", false, &["app-2.0.0-linux-x64.tar.gz"]),
				release(1, "v1.0.0", false, &["app-1.0.0-linux-x64.tar.gz"]),
			],
			true,
		);

		assert_eq!(
			chosen(resolve_curated(&project, &observation, LINUX, CAPS))
				.release
				.id,
			1
		);
	}

	#[test]
	fn two_packages_that_fit_equally_are_ambiguous_not_guessed() {
		let observation = observed(
			vec![release(
				1,
				"v1.0.0",
				false,
				&[
					"app-1.0.0-linux-x64.tar.gz",
					"app-1.0.0-linux-x64-extra.tar.gz",
				],
			)],
			true,
		);
		let project = project_with(
			vec![variant(
				"linux",
				"linux",
				"x86_64",
				"app-*linux-x64*.tar.gz",
			)],
			json!({}),
		);

		match resolve_curated(&project, &observation, LINUX, CAPS).outcome {
			Outcome::Ambiguous(candidates) => assert_eq!(candidates.len(), 2),
			other => panic!("expected ambiguity, got {other:?}"),
		}
	}

	#[test]
	fn excluded_files_and_unrelated_assets_never_match() {
		let observation = observed(
			vec![release(
				1,
				"v1.0.0",
				false,
				&[
					"app-1.0.0-linux-x64-debug.tar.gz",
					"app-1.0.0-linux-x64.tar.gz",
					"app-1.0.0-source.tar.gz",
				],
			)],
			true,
		);
		let project = project_with(
			vec![variant("linux", "linux", "x86_64", "app-*-x64*.tar.gz")],
			json!({}),
		);

		assert_eq!(
			chosen(resolve_curated(&project, &observation, LINUX, CAPS))
				.asset
				.name,
			"app-1.0.0-linux-x64.tar.gz"
		);
	}

	#[test]
	fn native_beats_universal_but_universal_alone_still_works() {
		let project = project_with(
			vec![
				variant("mac-universal", "macos", "universal", "app-*-universal.zip"),
				variant("mac-arm", "macos", "aarch64", "app-*-arm64.zip"),
			],
			json!({}),
		);

		let both = observed(
			vec![release(
				1,
				"v1.0.0",
				false,
				&["app-1.0.0-universal.zip", "app-1.0.0-arm64.zip"],
			)],
			true,
		);
		assert_eq!(
			chosen(resolve_curated(&project, &both, MAC, CAPS)).variant_id,
			"mac-arm"
		);

		let only_universal = observed(
			vec![release(1, "v1.0.0", false, &["app-1.0.0-universal.zip"])],
			true,
		);
		assert_eq!(
			chosen(resolve_curated(&project, &only_universal, MAC, CAPS)).variant_id,
			"mac-universal"
		);
	}

	#[test]
	fn reports_the_precise_reason_nothing_fits() {
		let observation = observed(
			vec![release(1, "v1.0.0", false, &["app-1.0.0-windows.zip"])],
			true,
		);
		let outcome = |project: &Project, host, caps: &[&str]| {
			resolve_curated(project, &observation, host, caps).outcome
		};

		assert_eq!(
			outcome(&linux_project(), MAC, CAPS),
			Outcome::NoCompatible(NoCompatible::NoVariantForDevice)
		);
		assert_eq!(
			outcome(&linux_project(), LINUX, &[]),
			Outcome::NoCompatible(NoCompatible::NeedsNewerLauncher)
		);
		assert_eq!(
			outcome(&linux_project(), LINUX, CAPS),
			Outcome::NoCompatible(NoCompatible::NoMatchingAsset { checked: 1 })
		);

		let none = observed(vec![release(1, "nightly", false, &[])], true);
		assert_eq!(
			resolve_curated(&linux_project(), &none, LINUX, CAPS).outcome,
			Outcome::NoCompatible(NoCompatible::NoEligibleRelease { checked: 1 })
		);
	}

	#[test]
	fn incomplete_paging_is_carried_into_the_result() {
		let observation = observed(vec![release(1, "v1.0.0", false, &[])], false);

		assert!(!resolve_curated(&linux_project(), &observation, LINUX, CAPS).discovery_complete);
	}

	#[test]
	fn paused_and_revoked_projects_resolve_to_nothing() {
		let observation = observed(
			vec![release(1, "v1.0.0", false, &["app-1.0.0-linux-x64.tar.gz"])],
			true,
		);

		for state in [State::Paused, State::Revoked] {
			let mut project = linux_project();
			project.status.state = state;

			assert_eq!(
				resolve_curated(&project, &observation, LINUX, CAPS).outcome,
				Outcome::Blocked(state)
			);
		}
	}

	#[test]
	fn opaque_versions_use_publication_time_and_say_so() {
		let project = project_with(
			vec![variant(
				"linux",
				"linux",
				"x86_64",
				"app-*-linux-x64.tar.gz",
			)],
			json!({"version_strategy": "opaque", "tag_include": ["build-*"]}),
		);
		let observation = observed(
			vec![
				release(1, "build-900", false, &["app-a-linux-x64.tar.gz"]),
				release(2, "build-12", false, &["app-b-linux-x64.tar.gz"]),
			],
			true,
		);

		let candidate = chosen(resolve_curated(&project, &observation, LINUX, CAPS));

		assert_eq!(
			candidate.release.tag, "build-12",
			"release 2 was published later"
		);
		assert_eq!(candidate.release.order, OrderBasis::ProviderOrder);
		assert!(
			candidate
				.explanation
				.iter()
				.any(|line| line.contains("no defined order"))
		);
	}

	#[test]
	fn unsupported_pattern_syntax_in_policy_is_refused() {
		let project = project_with(
			vec![variant("linux", "linux", "x86_64", "app-{a,b}.tar.gz")],
			json!({}),
		);
		let observation = observed(vec![release(1, "v1.0.0", false, &["app-a.tar.gz"])], true);

		assert_eq!(
			resolve_curated(&project, &observation, LINUX, CAPS).outcome,
			Outcome::NoCompatible(NoCompatible::InvalidRule)
		);
	}
}
