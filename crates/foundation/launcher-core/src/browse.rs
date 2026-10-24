//! Looking things up: releases, changelogs, platform support, and trust.
//!
//! Everything here is read-only. None of it changes what is installed.

use launcher_catalog::{Project, VerificationLevel};
use launcher_model::{ErrorCode, HostTarget, InstallationId, LauncherError};
use launcher_providers::{GithubProvider, Transport};
use launcher_resolver::{ReleaseView, list_releases};

use crate::engine::{Engine, now_unix};
use crate::prepare::pinned_identity;

/// Release notes made safe to print.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Changelog {
	pub version: String,
	pub text: String,
}

/// How one package type relates to this device.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VariantSupport {
	pub variant_id: String,
	pub os: String,
	pub arch: String,
	pub format: String,
	pub supported: bool,
	/// Why, in a sentence.
	pub reason: String,
}

#[derive(Clone, Debug)]
pub struct PlatformReport {
	/// `None` for a system the catalog has no name for.
	pub host: Option<HostTarget>,
	pub apps: Vec<(String, Vec<VariantSupport>)>,
}

#[derive(Clone, Debug)]
pub struct TrustStatus {
	pub feed_id: String,
	pub trusted_key_ids: Vec<String>,
	pub catalog_revision: Option<u64>,
	/// Keys whose signatures verified the catalog in use.
	pub verified_by: Vec<String>,
}

fn catalog_missing() -> LauncherError {
	LauncherError::new(
		ErrorCode::NetworkRequestFailed,
		"There is no verified catalog to look in.",
	)
}

impl Engine {
	fn project_of(
		&self,
		slug: &str,
	) -> Result<(std::sync::Arc<launcher_catalog::VerifiedCatalog>, String), LauncherError> {
		let catalog = self
			.catalog()
			.state()
			.catalog
			.clone()
			.ok_or_else(catalog_missing)?;

		if catalog.project(slug).is_none() {
			return Err(LauncherError::new(
				ErrorCode::InstallationNotFound,
				"That app is not in the catalog.",
			));
		}

		Ok((catalog, slug.to_owned()))
	}

	/// Every release GitHub lists for an app, with the verdict the launcher
	/// gives each.
	pub async fn releases(&self, slug: &str) -> Result<Vec<ReleaseView>, LauncherError> {
		let provider = GithubProvider::new(self.http.clone());

		self.releases_with(&provider, slug, HostTarget::current())
			.await
	}

	pub(crate) async fn releases_with<T: Transport>(
		&self,
		provider: &GithubProvider<T>,
		slug: &str,
		host: Option<HostTarget>,
	) -> Result<Vec<ReleaseView>, LauncherError> {
		let (catalog, slug) = self.project_of(slug)?;
		let project = catalog.project(&slug).ok_or_else(catalog_missing)?;
		let host = host.ok_or_else(|| {
			LauncherError::new(
				ErrorCode::PackageUnsupported,
				"This device type is not recognized.",
			)
		})?;

		let identity = pinned_identity(provider, project).await?;
		let observation = provider.releases(identity).await?;

		Ok(list_releases(
			project,
			&observation,
			host,
			self.catalog().config().supported_capabilities,
			None,
		))
	}

	/// The notes for one release, as plain text that is safe in a terminal.
	pub async fn changelog(
		&self,
		slug: &str,
		tag: Option<&str>,
	) -> Result<Changelog, LauncherError> {
		let provider = GithubProvider::new(self.http.clone());

		self.changelog_with(&provider, slug, tag).await
	}

	pub(crate) async fn changelog_with<T: Transport>(
		&self,
		provider: &GithubProvider<T>,
		slug: &str,
		tag: Option<&str>,
	) -> Result<Changelog, LauncherError> {
		let (catalog, slug) = self.project_of(slug)?;
		let project = catalog.project(&slug).ok_or_else(catalog_missing)?;
		let identity = pinned_identity(provider, project).await?;
		let observation = provider.releases(identity).await?;

		let release = match tag {
			Some(tag) => observation
				.releases
				.iter()
				.find(|release| release.tag == tag),
			None => observation
				.releases
				.iter()
				.find(|release| !release.prerelease),
		}
		.ok_or_else(|| {
			LauncherError::new(ErrorCode::ProviderNotFound, "No such release was found.")
		})?;

		Ok(Changelog {
			version: release.tag.clone(),
			text: sanitize_notes(release.notes.as_deref().unwrap_or("")),
		})
	}

	/// How an app's package types relate to this device, or every app's.
	pub fn platforms(&self, slug: Option<&str>) -> Result<PlatformReport, LauncherError> {
		let catalog = self
			.catalog()
			.state()
			.catalog
			.clone()
			.ok_or_else(catalog_missing)?;
		let host = HostTarget::current();
		let supported = self.catalog().config().supported_capabilities;

		let describe = |project: &Project| {
			project
				.variants
				.iter()
				.map(|variant| {
					let (supported_here, reason) = variant_support(variant, host, supported);

					VariantSupport {
						variant_id: variant.id.clone(),
						os: variant.os.label().to_owned(),
						arch: variant.arch.label().to_owned(),
						format: variant.format.label().to_owned(),
						supported: supported_here,
						reason,
					}
				})
				.collect::<Vec<_>>()
		};

		let apps = match slug {
			Some(slug) => {
				let project = catalog.project(slug).ok_or_else(|| {
					LauncherError::new(
						ErrorCode::InstallationNotFound,
						"That app is not in the catalog.",
					)
				})?;

				vec![(slug.to_owned(), describe(project))]
			}
			None => catalog
				.projects()
				.map(|(slug, project)| (slug.to_owned(), describe(project)))
				.collect(),
		};

		Ok(PlatformReport { host, apps })
	}

	/// The keys this build trusts, and which of them vouched for the catalog.
	pub fn trust_status(&self) -> TrustStatus {
		let config = self.catalog().config();
		let state = self.catalog().state();

		TrustStatus {
			feed_id: config.feed_id.clone(),
			trusted_key_ids: config
				.registry
				.active_key_ids()
				.into_iter()
				.map(str::to_owned)
				.collect(),
			catalog_revision: state.catalog.as_ref().map(|catalog| catalog.revision()),
			verified_by: state
				.catalog
				.as_ref()
				.map(|catalog| catalog.verified_by().to_vec())
				.unwrap_or_default(),
		}
	}

	/// What evidence an app's packages are checked against, in sentences.
	pub fn explain_trust(&self, slug: &str) -> Result<Vec<String>, LauncherError> {
		let (catalog, slug) = self.project_of(slug)?;
		let project = catalog.project(&slug).ok_or_else(catalog_missing)?;
		let mut lines = vec![
			format!(
				"{} is listed in catalog revision {}, signed by {}.",
				project.display.name,
				catalog.revision(),
				catalog.verified_by().join(", ")
			),
			format!(
				"Its source is {}, pinned by repository ID {} and owner ID {}.",
				project.source.display_name(),
				project.source.repository_id,
				project.source.owner_id
			),
		];

		for variant in &project.variants {
			let how = match variant.verification.minimum {
				VerificationLevel::HostDigest => "the SHA-256 GitHub reports for the file. That shows the file is intact, not who made it".to_owned(),
				VerificationLevel::PinnedSha256 => "a SHA-256 pinned in the signed catalog".to_owned(),
				VerificationLevel::PublisherSignature => "the publisher's own signature, which this launcher cannot check yet".to_owned(),
			};

			lines.push(format!(
				"{} {} ({}): checked against {how}.",
				variant.os.label(),
				variant.arch.label(),
				variant.format.label()
			));
		}

		lines.push(
			"A matching checksum or signature does not mean the software is harmless.".to_owned(),
		);

		Ok(lines)
	}

	/// Drops the launcher's record of an app and nothing else. The files stay
	/// where they are and the app keeps working; the launcher just stops
	/// managing it.
	pub async fn forget(&self, installation: InstallationId) -> Result<(), LauncherError> {
		let _lock = crate::lifecycle::acquire_operation_lock(&self.data).await?;
		let record = self
			.database
			.installation(installation)
			.await?
			.ok_or_else(|| {
				LauncherError::new(
					ErrorCode::InstallationNotFound,
					"That app is not installed.",
				)
			})?;

		// Only forget what is verifiably ours.
		crate::lifecycle::verify_ownership(&self.data.apps_dir(), &record)?;

		let now = now_unix();
		let operation = launcher_model::OperationId::new();

		self.database
			.begin_operation(launcher_storage::OperationRecord {
				id: operation,
				kind: "uninstall".into(),
				installation_id: Some(installation),
				plan_digest: [0; 32],
				plan: serde_json::json!({ "forget": installation.to_string(), "source": { "display": record.repository } }).to_string(),
				state: "removing".into(),
				staging: None,
				detail: None,
				created_at: now,
				updated_at: now,
			})
			.await?;

		self.database
			.commit_removal(
				operation,
				installation,
				"Forgotten. The files were left in place.".into(),
				now,
			)
			.await
	}
}

fn variant_support(
	variant: &launcher_catalog::Variant,
	host: Option<HostTarget>,
	supported: &[&str],
) -> (bool, String) {
	let Some(host) = host else {
		return (false, "This device type has no name in the catalog.".into());
	};

	if host.os != variant.os {
		return (
			false,
			format!(
				"Built for {}, and this is {}.",
				variant.os.label(),
				host.os.label()
			),
		);
	}

	if !host.runs(variant.os, variant.arch) {
		return (
			false,
			format!(
				"Built for {}, and this is {}.",
				variant.arch.label(),
				host.arch.label()
			),
		);
	}

	let missing: Vec<&str> = variant
		.required_capabilities
		.iter()
		.chain(&variant.install.required_capabilities)
		.map(String::as_str)
		.filter(|needed| !supported.contains(needed))
		.collect();

	if !missing.is_empty() {
		return (
			false,
			format!("Needs {}, which this launcher lacks.", missing.join(", ")),
		);
	}

	use launcher_catalog::PackageFormat::{TarGz, Zip};

	if !matches!(variant.format, Zip | TarGz) {
		return (
			false,
			format!(
				"{} packages cannot be installed by the launcher yet.",
				variant.format.label()
			),
		);
	}

	(true, "Runs here and can be installed.".into())
}

/// Turns release notes into plain text that is safe to print.
///
/// Notes are written by whoever publishes the release, so they are data. HTML
/// tags are removed rather than interpreted, and every control character,
/// including the escape that starts a terminal escape sequence, is dropped so
/// notes cannot move the cursor, change colors, or set a window title.
pub(crate) fn sanitize_notes(notes: &str) -> String {
	const LIMIT: usize = 20_000;

	let mut text = String::with_capacity(notes.len().min(LIMIT));
	let mut inside_tag = false;

	for character in notes.chars().take(LIMIT) {
		match character {
			'<' => inside_tag = true,
			'>' if inside_tag => inside_tag = false,
			_ if inside_tag => {}
			'\n' | '\t' => text.push(character),
			'\r' => {}
			c if c.is_control() => {}
			// Bidirectional overrides can make text read differently than it
			// is written.
			'\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}' | '\u{200e}' | '\u{200f}' => {}
			c => text.push(c),
		}
	}

	// A tag that was never closed swallowed the rest, which is the safe way to fail.
	let mut collapsed = String::with_capacity(text.len());
	let mut blank_run = 0;

	for line in text.lines() {
		if line.trim().is_empty() {
			blank_run += 1;

			if blank_run > 1 {
				continue;
			}
		} else {
			blank_run = 0;
		}

		collapsed.push_str(line.trim_end());
		collapsed.push('\n');
	}

	collapsed.trim().to_owned()
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn html_is_removed_not_interpreted() {
		assert_eq!(
			sanitize_notes("Fixed <b>bugs</b> <script>alert(1)</script> and more"),
			"Fixed bugs alert(1) and more"
		);
		assert_eq!(sanitize_notes("a <img src=x onerror=1> b"), "a  b");
	}

	#[test]
	fn terminal_escapes_and_control_characters_are_dropped() {
		let hostile = "ok\u{1b}[2J\u{1b}]0;pwned\u{7}\u{0}end\r\n";

		assert_eq!(sanitize_notes(hostile), "ok[2J]0;pwnedend");
		assert!(
			!sanitize_notes(hostile)
				.chars()
				.any(|c| c.is_control() && c != '\n')
		);
	}

	#[test]
	fn direction_overrides_are_dropped() {
		assert_eq!(sanitize_notes("safe\u{202e}txt.exe"), "safetxt.exe");
	}

	#[test]
	fn blank_runs_collapse_and_lines_are_trimmed() {
		assert_eq!(sanitize_notes("one  \n\n\n\ntwo\n"), "one\n\ntwo");
		assert_eq!(sanitize_notes(""), "");
	}

	#[test]
	fn an_unclosed_tag_swallows_the_rest_and_length_is_bounded() {
		assert_eq!(sanitize_notes("before <never closed"), "before");
		assert!(sanitize_notes(&"x".repeat(50_000)).len() <= 20_000);
	}
}
