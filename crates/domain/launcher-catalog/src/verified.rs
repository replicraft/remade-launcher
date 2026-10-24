use std::sync::OnceLock;

use rayon::prelude::*;
use time::OffsetDateTime;

use crate::manifest::{Manifest, Project};

/// Whether a verified catalog may still authorize new installs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Freshness {
	Current,
	/// Past `expires_at`. Browsing and local lifecycle actions still work.
	Expired,
}

/// A catalog whose signature, structure, and revision all checked out.
///
/// There is no public constructor. The only way to get one is
/// [`crate::verify`], so holding a value is proof of verification. The raw
/// bytes are kept so they can be stored and later re-verified: the stored
/// copy is never trusted on its own.
#[derive(Debug)]
pub struct VerifiedCatalog {
	pub(crate) manifest: Manifest,
	pub(crate) manifest_bytes: Vec<u8>,
	pub(crate) envelope_bytes: Vec<u8>,
	pub(crate) digest: [u8; 32],
	pub(crate) issued_at: OffsetDateTime,
	pub(crate) expires_at: OffsetDateTime,
	pub(crate) verified_by: Vec<String>,
	/// Lowercase search text per project, built the first time it is needed.
	pub(crate) search: OnceLock<Vec<String>>,
}

impl VerifiedCatalog {
	pub fn feed_id(&self) -> &str {
		&self.manifest.feed_id
	}

	pub fn revision(&self) -> u64 {
		self.manifest.revision
	}

	/// SHA-256 of the exact manifest bytes. Two manifests with the same
	/// revision must have the same digest.
	pub fn digest(&self) -> [u8; 32] {
		self.digest
	}

	pub fn issued_at(&self) -> OffsetDateTime {
		self.issued_at
	}

	pub fn expires_at(&self) -> OffsetDateTime {
		self.expires_at
	}

	/// IDs of the trusted keys whose signatures verified.
	pub fn verified_by(&self) -> &[String] {
		&self.verified_by
	}

	pub fn manifest_bytes(&self) -> &[u8] {
		&self.manifest_bytes
	}

	pub fn envelope_bytes(&self) -> &[u8] {
		&self.envelope_bytes
	}

	pub fn freshness(&self, now: OffsetDateTime) -> Freshness {
		if now < self.expires_at {
			Freshness::Current
		} else {
			Freshness::Expired
		}
	}

	/// Projects in catalog (slug) order.
	pub fn projects(&self) -> impl ExactSizeIterator<Item = (&str, &Project)> {
		self.manifest
			.projects
			.iter()
			.map(|(slug, project)| (slug.as_str(), project))
	}

	/// What each project can be found by, lowercase, in [`Self::projects`]
	/// order: its slug, name, summary, owner, repository, categories, and the
	/// names of the original software it remakes.
	///
	/// It is built once, in parallel, the first time it is asked for. Typing in
	/// a search box then costs a scan, not thousands of small allocations.
	pub fn search_texts(&self) -> &[String] {
		self.search.get_or_init(|| {
			let projects: Vec<_> = self.projects().collect();

			projects
				.par_iter()
				.map(|(slug, project)| {
					let display = &project.display;
					let mut parts = vec![
						*slug,
						display.name.as_str(),
						display.summary.as_str(),
						project.source.owner.as_str(),
						project.source.repository.as_str(),
					];

					parts.extend(display.categories.iter().map(String::as_str));
					parts.extend(display.original_names.iter().map(String::as_str));

					parts.join("\n").to_lowercase()
				})
				.collect()
		})
	}

	pub fn project(&self, slug: &str) -> Option<&Project> {
		self.manifest.projects.get(slug)
	}

	pub fn project_count(&self) -> usize {
		self.manifest.projects.len()
	}
}
