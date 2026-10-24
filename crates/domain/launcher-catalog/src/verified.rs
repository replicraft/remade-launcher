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

	pub fn project(&self, slug: &str) -> Option<&Project> {
		self.manifest.projects.get(slug)
	}

	pub fn project_count(&self) -> usize {
		self.manifest.projects.len()
	}
}
