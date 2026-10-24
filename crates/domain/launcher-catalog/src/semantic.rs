//! Checks that the structure alone cannot express.
//!
//! The schema says what shape a field has. These checks say what it may
//! contain: safe relative paths, consistent platform tuples, complete
//! verification policy, and unique identifiers. Projects are independent, so
//! they are checked in parallel with rayon, then the first failure in
//! catalog order is reported so the result is deterministic.

use std::collections::HashSet;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use launcher_model::{Arch, Os};
use rayon::prelude::*;

use crate::envelope::is_identifier;
use crate::error::{CatalogError, ErrorKind};
use crate::limits::{MAX_PROJECTS, MAX_VARIANTS};
use crate::manifest::{Manifest, Project, PublisherFormat, Tracking, Variant, VerificationLevel};

pub(crate) fn validate(manifest: &Manifest) -> Result<(), CatalogError> {
	if manifest.projects.len() > MAX_PROJECTS {
		return Err(schema("the catalog lists too many projects"));
	}

	if manifest.revision == 0 || manifest.revision > crate::limits::MAX_SAFE_INTEGER {
		return Err(schema("the revision is out of range"));
	}

	if !is_identifier(&manifest.feed_id) {
		return Err(schema("the feed ID is malformed"));
	}

	check_capability_list(&manifest.required_capabilities)?;

	let projects: Vec<_> = manifest.projects.iter().collect();

	// `collect` keeps input order, so the first error is stable across runs.
	let results: Vec<Result<(), CatalogError>> = projects
		.par_iter()
		.map(|(slug, project)| validate_project(slug, project))
		.collect();

	results.into_iter().collect()
}

fn validate_project(slug: &str, project: &Project) -> Result<(), CatalogError> {
	if !is_slug(slug) {
		return Err(schema(format!("project slug {slug:?} is malformed")));
	}

	let display = &project.display;

	check_text(&display.name, 160, "name")?;
	check_text(&display.summary, 500, "summary")?;

	if display.categories.len() > 16 || !display.categories.iter().all(|c| is_slug(c)) {
		return Err(schema("categories are malformed"));
	}

	let links = [
		&display.links.homepage,
		&display.links.documentation,
		&display.links.source,
		&display.links.changelog,
		&display.links.community,
		&display.links.license,
		&display.icon_url,
		&project.status.advisory_url,
	];

	for link in links.into_iter().flatten().chain(&display.screenshots) {
		check_https_url(link)?;
	}

	if display.screenshots.len() > 20 {
		return Err(schema("too many screenshots"));
	}

	if let Some(description) = &display.description {
		check_long_text(description, 8_000, "description")?;
	}

	if display.languages.len() > 8 {
		return Err(schema("too many languages"));
	}

	for language in &display.languages {
		check_text(language, 40, "language")?;
	}

	if display.added_at.is_some() && display.added_at_unix().is_none() {
		return Err(schema("added_at is not a UTC whole-second timestamp"));
	}

	check_numeric_id(&project.source.repository_id)?;
	check_numeric_id(&project.source.owner_id)?;
	check_text(&project.source.owner, 100, "owner")?;
	check_text(&project.source.repository, 100, "repository")?;

	let policy = &project.release_policy;

	if policy.tag_include.is_empty()
		|| policy.tag_include.len() > 16
		|| policy.tag_exclude.len() > 16
	{
		return Err(schema("tag filters have an unsupported size"));
	}

	if policy.tracking == Tracking::Pinned {
		match &policy.pinned_release_id {
			Some(id) => check_numeric_id(id)?,
			None => return Err(schema("a pinned project needs a release ID")),
		}
	}

	if project.variants.len() > MAX_VARIANTS {
		return Err(schema("a project has too many variants"));
	}

	let mut ids = HashSet::new();

	for variant in &project.variants {
		if !ids.insert(variant.id.as_str()) {
			return Err(CatalogError::new(
				ErrorKind::DuplicateVariant,
				format!("variant {} appears twice in project {slug}", variant.id),
			));
		}

		validate_variant(variant)?;
	}

	Ok(())
}

fn validate_variant(variant: &Variant) -> Result<(), CatalogError> {
	if !is_identifier(&variant.id) {
		return Err(schema("a variant ID is malformed"));
	}

	if variant.arch == Arch::Universal && variant.os != Os::Macos {
		return Err(schema("the universal architecture only exists on macOS"));
	}

	check_capability_list(&variant.required_capabilities)?;
	check_capability_list(&variant.install.required_capabilities)?;
	check_selectors(&variant.assets.include, &variant.assets.exclude, 32)?;

	if !is_identifier(&variant.install.adapter) {
		return Err(schema("an adapter ID is malformed"));
	}

	for path in [
		&variant.install.entry_point,
		&variant.install.bundle_locator,
	]
	.into_iter()
	.flatten()
	{
		check_relative_path(path)?;
	}

	if variant
		.install
		.strip_components
		.is_some_and(|count| count > 8)
	{
		return Err(schema("strip_components is out of range"));
	}

	let verification = &variant.verification;

	match verification.minimum {
		VerificationLevel::HostDigest => {}
		VerificationLevel::PinnedSha256 => {
			let digest = verification.sha256.as_deref();

			if !digest.is_some_and(is_sha256_hex) {
				return Err(schema("pinned verification needs a SHA-256 digest"));
			}
		}
		VerificationLevel::PublisherSignature => {
			let policy = verification
				.publisher_policy
				.as_ref()
				.ok_or_else(|| schema("publisher verification needs a policy"))?;

			check_text(&policy.identity, 1024, "publisher identity")?;

			if policy.format == PublisherFormat::Ed25519Detached {
				let key = policy.public_key_base64.as_deref().unwrap_or_default();
				let key_ok = STANDARD.decode(key).is_ok_and(|bytes| bytes.len() == 32);
				let assets = policy
					.signature_assets
					.as_ref()
					.ok_or_else(|| schema("a detached signature policy needs signature assets"))?;

				if !key_ok {
					return Err(schema("the publisher public key is malformed"));
				}

				check_selectors(&assets.include, &assets.exclude, 16)?;
			}
		}
	}

	Ok(())
}

fn schema(detail: impl Into<String>) -> CatalogError {
	CatalogError::new(ErrorKind::SchemaInvalid, detail)
}

/// Like [`check_text`], but newlines may separate paragraphs.
fn check_long_text(text: &str, max: usize, what: &str) -> Result<(), CatalogError> {
	if text.trim().is_empty()
		|| text.chars().count() > max
		|| text.chars().any(|c| c.is_control() && c != '\n')
	{
		return Err(schema(format!(
			"{what} is empty, too long, or has control characters"
		)));
	}

	Ok(())
}

fn check_text(text: &str, max: usize, what: &str) -> Result<(), CatalogError> {
	// Counted in characters. The schema's limits are in characters too.
	if text.is_empty() || text.chars().count() > max || text.chars().any(char::is_control) {
		return Err(schema(format!(
			"{what} is empty, too long, or has control characters"
		)));
	}

	Ok(())
}

fn check_https_url(url: &str) -> Result<(), CatalogError> {
	if url.len() > 2048
		|| !url.starts_with("https://")
		|| url.chars().any(|c| c.is_control() || c == ' ')
	{
		return Err(schema("a link is not a plain https URL"));
	}

	Ok(())
}

fn check_numeric_id(text: &str) -> Result<(), CatalogError> {
	let valid = (1..=20).contains(&text.len())
		&& !text.starts_with('0')
		&& text.bytes().all(|byte| byte.is_ascii_digit());

	if valid {
		Ok(())
	} else {
		Err(schema("an identifier is not a positive decimal number"))
	}
}

fn check_capability_list(list: &[String]) -> Result<(), CatalogError> {
	let unique: HashSet<_> = list.iter().collect();

	if list.len() > 64 || unique.len() != list.len() || !list.iter().all(|c| is_identifier(c)) {
		return Err(schema("a capability list is malformed"));
	}

	Ok(())
}

fn check_selectors(
	include: &[String],
	exclude: &[String],
	max_exclude: usize,
) -> Result<(), CatalogError> {
	let sized = !include.is_empty() && include.len() <= 16 && exclude.len() <= max_exclude;
	let patterns_ok = include
		.iter()
		.chain(exclude)
		.all(|p| !p.is_empty() && p.len() <= 256 && !p.contains('\0'));

	if sized && patterns_ok {
		Ok(())
	} else {
		Err(schema("an asset selector is malformed"))
	}
}

/// Wire-format relative paths: forward slashes, no empty, `.` or `..`
/// components, no drive letters, no control characters.
pub(crate) fn check_relative_path(path: &str) -> Result<(), CatalogError> {
	let bad_character = |c: char| c == '\\' || c == ':' || c.is_control();
	let bad_component = |part: &str| part.is_empty() || part == "." || part == "..";

	let valid = !path.is_empty()
		&& path.len() <= 4096
		&& !path.starts_with('/')
		&& !path.chars().any(bad_character)
		&& !path.split('/').any(bad_component);

	if valid {
		Ok(())
	} else {
		Err(schema(format!("path {path:?} is not a safe relative path")))
	}
}

/// `^[a-z0-9]+(?:-[a-z0-9]+)*$`, at most 100 characters.
fn is_slug(text: &str) -> bool {
	!text.is_empty()
		&& text.len() <= 100
		&& text.split('-').all(|part| {
			!part.is_empty()
				&& part
					.bytes()
					.all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
		})
}

fn is_sha256_hex(text: &str) -> bool {
	text.len() == 64
		&& text
			.bytes()
			.all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn safe_paths_pass() {
		for path in ["bin/app", "App.app", "a/b/c.exe", "with space/ok"] {
			assert!(check_relative_path(path).is_ok(), "{path}");
		}
	}

	#[test]
	fn unsafe_paths_fail() {
		for path in [
			"", "/abs", "../up", "a/../b", "a/./b", "a//b", "a/", "C:\\x", "c:/x", "a\\b", "a\0b",
			"a\nb", ".", "..",
		] {
			assert!(check_relative_path(path).is_err(), "{path:?}");
		}
	}

	#[test]
	fn long_text_allows_paragraph_breaks_but_no_other_control_characters() {
		assert!(check_long_text("One.\n\nTwo.", 100, "description").is_ok());

		for bad in ["", "   ", "tab\there", "bell\u{7}", &"x".repeat(101)] {
			assert!(check_long_text(bad, 100, "description").is_err(), "{bad:?}");
		}
	}

	#[test]
	fn slugs_follow_the_contract() {
		assert!(is_slug("sample-editor"));
		assert!(is_slug("a1-b2"));

		for bad in ["", "Sample", "-a", "a-", "a--b", "a_b", "../x"] {
			assert!(!is_slug(bad), "{bad}");
		}
	}

	#[test]
	fn numeric_ids_reject_leading_zero_and_signs() {
		assert!(check_numeric_id("100001").is_ok());

		for bad in ["", "0", "01", "-1", "1e3", "123456789012345678901"] {
			assert!(check_numeric_id(bad).is_err(), "{bad}");
		}
	}

	#[test]
	fn only_https_links_pass() {
		assert!(check_https_url("https://example.com/a").is_ok());
		assert!(check_https_url("http://example.com").is_err());
		assert!(check_https_url("javascript:alert(1)").is_err());
		assert!(check_https_url("https://a b").is_err());
	}
}
