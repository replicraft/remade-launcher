//! The verification pipeline.

use ed25519_dalek::Signature;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use time::{Duration, OffsetDateTime};

use crate::envelope::{self, SignatureEntry};
use crate::error::{CatalogError, ErrorKind};
use crate::limits::{MAX_CLOCK_SKEW_MINUTES, MAX_LIFETIME_DAYS, MAX_MANIFEST_BYTES};
use crate::manifest::Manifest;
use crate::registry::KeyRegistry;
use crate::semantic;
use crate::strict_json::{self, StrictError};
use crate::timestamp;
use crate::verified::VerifiedCatalog;

/// What this build already accepted for the feed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HighWater {
	pub revision: u64,
	/// SHA-256 of the accepted manifest bytes, when known.
	pub digest: Option<[u8; 32]>,
}

/// How to treat a catalog past its expiry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FreshnessPolicy {
	/// Reject expired catalogs. Use for anything fetched from the network.
	RequireCurrent,
	/// Accept them. Use when re-verifying our own cache, which stays useful
	/// for browsing after it expires.
	AllowExpired,
}

pub struct VerifyContext<'a> {
	pub registry: &'a KeyRegistry,
	/// The feed this client is configured for.
	pub feed_id: &'a str,
	/// A fixed time for the whole attempt. Never an HTTP `Date` header.
	pub now: OffsetDateTime,
	/// `None` when re-verifying cached bytes against themselves.
	pub high_water: Option<HighWater>,
	pub supported_capabilities: &'a [&'a str],
	pub freshness: FreshnessPolicy,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Change {
	/// A newer revision than we had.
	New,
	/// The identical manifest we already accepted.
	Unchanged,
}

pub struct Verification {
	pub catalog: VerifiedCatalog,
	pub change: Change,
}

/// Verifies a manifest and its detached signature envelope.
pub fn verify(
	manifest_bytes: &[u8],
	envelope_bytes: &[u8],
	context: &VerifyContext<'_>,
) -> Result<Verification, CatalogError> {
	if manifest_bytes.len() > MAX_MANIFEST_BYTES {
		return Err(CatalogError::new(
			ErrorKind::InvalidJson,
			"the manifest is too large",
		));
	}

	let entries = envelope::parse(envelope_bytes)?;
	let verified_by = verify_signatures(manifest_bytes, &entries, context.registry)?;

	// Authentic bytes can still be malformed, so parsing comes second.
	let value = strict_json::parse(manifest_bytes).map_err(|error| match error {
		StrictError::DuplicateKey => CatalogError::new(
			ErrorKind::DuplicateJsonKey,
			"the manifest repeats an object key",
		),
		StrictError::Invalid(detail) => CatalogError::new(ErrorKind::InvalidJson, detail),
	})?;

	let manifest = Manifest::deserialize(&value)
		.map_err(|error| CatalogError::new(ErrorKind::SchemaInvalid, error.to_string()))?;

	if manifest.schema_version != 1 {
		return Err(CatalogError::new(
			ErrorKind::SchemaInvalid,
			format!("unsupported schema version {}", manifest.schema_version),
		));
	}

	if manifest.feed_id != context.feed_id || manifest.feed_id != context.registry.feed_id() {
		return Err(CatalogError::new(
			ErrorKind::WrongFeed,
			"the manifest is for another feed",
		));
	}

	let (issued_at, expires_at) = check_times(&manifest, context)?;
	check_capabilities(&manifest, context.supported_capabilities)?;

	// Structural and semantic policy, in parallel across projects.
	semantic::validate(&manifest)?;

	check_projects_hash(&manifest, &value)?;

	let digest: [u8; 32] = Sha256::digest(manifest_bytes).into();
	let change = check_revision(manifest.revision, digest, context.high_water)?;

	Ok(Verification {
		catalog: VerifiedCatalog {
			manifest,
			manifest_bytes: manifest_bytes.to_vec(),
			envelope_bytes: envelope_bytes.to_vec(),
			digest,
			issued_at,
			expires_at,
			verified_by,
		},
		change,
	})
}

/// Accepts the manifest when at least one locally active key signed it.
///
/// This is a one-of-N policy, not a quorum. Unknown key IDs are skipped, and
/// an invalid signature beside a valid trusted one does not poison the set.
fn verify_signatures(
	message: &[u8],
	entries: &[SignatureEntry],
	registry: &KeyRegistry,
) -> Result<Vec<String>, CatalogError> {
	let mut verified = Vec::new();
	let mut tried_a_trusted_key = false;

	for entry in entries {
		let Some(trusted) = registry.active_key(&entry.key_id) else {
			continue;
		};

		tried_a_trusted_key = true;

		if signature_is_valid(&trusted.key, message, &entry.signature) {
			verified.push(entry.key_id.clone());
		}
	}

	if !verified.is_empty() {
		Ok(verified)
	} else if tried_a_trusted_key {
		Err(CatalogError::new(
			ErrorKind::SignatureInvalid,
			"no signature matched the manifest bytes",
		))
	} else {
		Err(CatalogError::new(
			ErrorKind::NoTrustedSignature,
			"no signature came from a trusted key",
		))
	}
}

fn signature_is_valid(
	key: &ed25519_dalek::VerifyingKey,
	message: &[u8],
	signature: &Signature,
) -> bool {
	// `verify_strict` also rejects small-order keys and non-canonical
	// signatures, which plain `verify` accepts.
	key.verify_strict(message, signature).is_ok()
}

fn check_times(
	manifest: &Manifest,
	context: &VerifyContext<'_>,
) -> Result<(OffsetDateTime, OffsetDateTime), CatalogError> {
	let parse = |text: &str, what: &str| {
		timestamp::parse(text).ok_or_else(|| {
			CatalogError::new(
				ErrorKind::SchemaInvalid,
				format!("{what} is not a UTC whole-second timestamp"),
			)
		})
	};

	let issued_at = parse(&manifest.issued_at, "issued_at")?;
	let expires_at = parse(&manifest.expires_at, "expires_at")?;

	if expires_at <= issued_at || expires_at - issued_at > Duration::days(MAX_LIFETIME_DAYS) {
		return Err(CatalogError::new(
			ErrorKind::InvalidTimeOrder,
			"the validity window is empty or longer than policy allows",
		));
	}

	if issued_at > context.now + Duration::minutes(MAX_CLOCK_SKEW_MINUTES) {
		return Err(CatalogError::new(
			ErrorKind::FutureIssued,
			"the manifest was issued in the future",
		));
	}

	if context.freshness == FreshnessPolicy::RequireCurrent && expires_at <= context.now {
		return Err(CatalogError::new(
			ErrorKind::Expired,
			"the manifest has expired",
		));
	}

	Ok((issued_at, expires_at))
}

fn check_capabilities(manifest: &Manifest, supported: &[&str]) -> Result<(), CatalogError> {
	let missing = manifest
		.required_capabilities
		.iter()
		.find(|required| !supported.contains(&required.as_str()));

	match missing {
		Some(capability) => Err(CatalogError::new(
			ErrorKind::UnsupportedCapability,
			format!("the feed requires capability {capability:?}, which this build lacks"),
		)),
		None => Ok(()),
	}
}

/// Recomputes SHA-256 over the RFC 8785 canonical form of `projects`.
fn check_projects_hash(manifest: &Manifest, value: &serde_json::Value) -> Result<(), CatalogError> {
	let projects = value.get("projects").ok_or_else(|| {
		CatalogError::new(ErrorKind::SchemaInvalid, "the manifest has no projects")
	})?;

	let canonical = serde_json_canonicalizer::to_vec(projects)
		.map_err(|error| CatalogError::new(ErrorKind::SchemaInvalid, error.to_string()))?;
	let actual = hex::encode(Sha256::digest(canonical));

	if actual == manifest.hash {
		Ok(())
	} else {
		Err(CatalogError::new(
			ErrorKind::HashMismatch,
			"the projects hash does not match",
		))
	}
}

fn check_revision(
	revision: u64,
	digest: [u8; 32],
	high_water: Option<HighWater>,
) -> Result<Change, CatalogError> {
	let Some(known) = high_water else {
		return Ok(Change::New);
	};

	match revision.cmp(&known.revision) {
		std::cmp::Ordering::Greater => Ok(Change::New),
		std::cmp::Ordering::Less => Err(CatalogError::new(
			ErrorKind::Rollback,
			format!(
				"revision {revision} is older than the accepted {}",
				known.revision
			),
		)),
		std::cmp::Ordering::Equal if known.digest == Some(digest) => Ok(Change::Unchanged),
		std::cmp::Ordering::Equal => Err(CatalogError::new(
			ErrorKind::RevisionConflict,
			"the same revision was published with different bytes",
		)),
	}
}
