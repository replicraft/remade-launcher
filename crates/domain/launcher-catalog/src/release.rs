//! The signed description of a launcher release.
//!
//! The launcher updates itself from one small signed document: which version
//! is current, and for each system the file to download, its size, and its
//! SHA-256. It is checked the way the catalog is, with a key list of its own,
//! so a catalog key can never sign a launcher and a launcher key can never
//! sign a catalog.

use std::collections::HashSet;

use serde::Deserialize;
use sha2::{Digest, Sha256};
use time::{Duration, OffsetDateTime};

use crate::envelope;
use crate::error::{CatalogError, ErrorKind};
use crate::limits::{MAX_CLOCK_SKEW_MINUTES, MAX_LIFETIME_DAYS, MAX_MANIFEST_BYTES};
use crate::registry::KeyRegistry;
use crate::strict_json::{self, StrictError};
use crate::timestamp;
use crate::verify::{HighWater, verify_signatures};

/// The largest file a release may ask the launcher to download.
pub const MAX_RELEASE_BYTES: u64 = 512 * 1024 * 1024;

const MAX_ARTIFACTS: usize = 32;

/// The release feed as published.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawRelease {
	schema_version: u32,
	feed_id: String,
	channel: String,
	revision: u64,
	issued_at: String,
	expires_at: String,
	version: String,
	#[serde(default)]
	notes_url: Option<String>,
	artifacts: Vec<ReleaseArtifact>,
}

/// One downloadable file of a release.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ReleaseArtifact {
	pub os: String,
	pub arch: String,
	/// The file's name, for display and for the saved copy.
	pub name: String,
	pub url: String,
	/// Lowercase hex SHA-256 of the file.
	pub sha256: String,
	pub size: u64,
}

/// A release whose signature, structure, and freshness checked out.
///
/// There is no public constructor: [`verify_release`] is the only way to get
/// one.
#[derive(Debug)]
pub struct VerifiedRelease {
	version: semver::Version,
	channel: String,
	revision: u64,
	notes_url: Option<String>,
	artifacts: Vec<ReleaseArtifact>,
	digest: [u8; 32],
	issued_at: OffsetDateTime,
	expires_at: OffsetDateTime,
	verified_by: Vec<String>,
}

impl VerifiedRelease {
	pub fn version(&self) -> &semver::Version {
		&self.version
	}

	pub fn channel(&self) -> &str {
		&self.channel
	}

	pub fn revision(&self) -> u64 {
		self.revision
	}

	/// Where the release notes are, an https address.
	pub fn notes_url(&self) -> Option<&str> {
		self.notes_url.as_deref()
	}

	pub fn artifacts(&self) -> &[ReleaseArtifact] {
		&self.artifacts
	}

	/// The file for this system, if the release has one.
	pub fn artifact_for(&self, os: &str, arch: &str) -> Option<&ReleaseArtifact> {
		self.artifacts
			.iter()
			.find(|artifact| artifact.os == os && artifact.arch == arch)
	}

	pub fn digest(&self) -> [u8; 32] {
		self.digest
	}

	pub fn issued_at(&self) -> OffsetDateTime {
		self.issued_at
	}

	pub fn expires_at(&self) -> OffsetDateTime {
		self.expires_at
	}

	pub fn verified_by(&self) -> &[String] {
		&self.verified_by
	}
}

pub struct ReleaseContext<'a> {
	pub registry: &'a KeyRegistry,
	pub feed_id: &'a str,
	/// Which channel this launcher follows.
	pub channel: &'a str,
	pub now: OffsetDateTime,
	/// What this build already accepted, so an older document cannot replace
	/// a newer one.
	pub high_water: Option<HighWater>,
}

/// Checks a release document and its detached signatures.
pub fn verify_release(
	manifest_bytes: &[u8],
	envelope_bytes: &[u8],
	context: &ReleaseContext<'_>,
) -> Result<VerifiedRelease, CatalogError> {
	// A release document is small. The catalog's ceiling is far too generous.
	if manifest_bytes.len() > MAX_MANIFEST_BYTES.min(256 * 1024) {
		return Err(CatalogError::new(
			ErrorKind::InvalidJson,
			"the release document is too large",
		));
	}

	let entries = envelope::parse(envelope_bytes)?;
	let verified_by = verify_signatures(manifest_bytes, &entries, context.registry)?;

	let value = strict_json::parse(manifest_bytes).map_err(|error| match error {
		StrictError::DuplicateKey => CatalogError::new(
			ErrorKind::DuplicateJsonKey,
			"the release document repeats an object key",
		),
		StrictError::Invalid(detail) => CatalogError::new(ErrorKind::InvalidJson, detail),
	})?;
	let raw = RawRelease::deserialize(&value)
		.map_err(|error| CatalogError::new(ErrorKind::SchemaInvalid, error.to_string()))?;

	let schema = |detail: &str| CatalogError::new(ErrorKind::SchemaInvalid, detail.to_owned());

	if raw.schema_version != 1 {
		return Err(schema("unsupported release schema version"));
	}

	if raw.feed_id != context.feed_id || raw.feed_id != context.registry.feed_id() {
		return Err(CatalogError::new(
			ErrorKind::WrongFeed,
			"the release document is for another feed",
		));
	}

	if raw.channel != context.channel {
		return Err(schema("the release is for another channel"));
	}

	let issued_at = parse_time(&raw.issued_at, "issued_at")?;
	let expires_at = parse_time(&raw.expires_at, "expires_at")?;

	if expires_at <= issued_at || expires_at - issued_at > Duration::days(MAX_LIFETIME_DAYS) {
		return Err(CatalogError::new(
			ErrorKind::InvalidTimeOrder,
			"the validity window is empty or longer than policy allows",
		));
	}

	if issued_at > context.now + Duration::minutes(MAX_CLOCK_SKEW_MINUTES) {
		return Err(CatalogError::new(
			ErrorKind::FutureIssued,
			"the release was issued in the future",
		));
	}

	// An expired document is refused, so an attacker cannot keep serving an
	// old, vulnerable "latest" release forever.
	if expires_at <= context.now {
		return Err(CatalogError::new(
			ErrorKind::Expired,
			"the release document has expired",
		));
	}

	let version = semver::Version::parse(&raw.version)
		.map_err(|_| schema("the version is not a semantic version"))?;

	if !version.pre.is_empty() {
		return Err(schema("a pre-release cannot be the current release"));
	}

	if let Some(url) = &raw.notes_url
		&& !is_https(url)
	{
		return Err(schema("the release notes address is not https"));
	}

	check_artifacts(&raw.artifacts)?;

	let digest: [u8; 32] = Sha256::digest(manifest_bytes).into();

	if let Some(known) = context.high_water {
		match raw.revision.cmp(&known.revision) {
			std::cmp::Ordering::Less => {
				return Err(CatalogError::new(
					ErrorKind::Rollback,
					format!(
						"revision {} is older than the accepted {}",
						raw.revision, known.revision
					),
				));
			}
			std::cmp::Ordering::Equal if known.digest.is_some_and(|d| d != digest) => {
				return Err(CatalogError::new(
					ErrorKind::RevisionConflict,
					"the same revision was published with different bytes",
				));
			}
			_ => {}
		}
	}

	Ok(VerifiedRelease {
		version,
		channel: raw.channel,
		revision: raw.revision,
		notes_url: raw.notes_url,
		artifacts: raw.artifacts,
		digest,
		issued_at,
		expires_at,
		verified_by,
	})
}

fn parse_time(text: &str, what: &str) -> Result<OffsetDateTime, CatalogError> {
	timestamp::parse(text).ok_or_else(|| {
		CatalogError::new(
			ErrorKind::SchemaInvalid,
			format!("{what} is not a UTC whole-second timestamp"),
		)
	})
}

fn is_https(url: &str) -> bool {
	url.len() <= 2048
		&& url.starts_with("https://")
		&& !url.chars().any(|c| c.is_control() || c.is_whitespace())
}

/// A file can only come from this project's GitHub releases.
fn is_release_download(url: &str) -> bool {
	is_https(url)
		&& url
			.strip_prefix("https://github.com/")
			.is_some_and(|rest| rest.contains("/releases/download/") && !rest.contains(".."))
}

fn check_artifacts(artifacts: &[ReleaseArtifact]) -> Result<(), CatalogError> {
	let schema = |detail: String| CatalogError::new(ErrorKind::SchemaInvalid, detail);

	if artifacts.is_empty() || artifacts.len() > MAX_ARTIFACTS {
		return Err(schema(
			"the release has an unsupported number of files".into(),
		));
	}

	let mut seen = HashSet::new();

	for artifact in artifacts {
		let known_os = matches!(artifact.os.as_str(), "macos" | "windows" | "linux");
		let known_arch = matches!(artifact.arch.as_str(), "x86_64" | "aarch64");

		if !known_os || !known_arch {
			return Err(schema(format!(
				"{}/{} is not a system this launcher knows",
				artifact.os, artifact.arch
			)));
		}

		if !seen.insert((&artifact.os, &artifact.arch)) {
			return Err(schema("two files claim the same system".into()));
		}

		let name_ok = !artifact.name.is_empty()
			&& artifact.name.len() <= 200
			&& !artifact.name.starts_with('.')
			&& artifact
				.name
				.chars()
				.all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'));

		if !name_ok {
			return Err(schema("a file name is not safe to save".into()));
		}

		if !is_release_download(&artifact.url) {
			return Err(schema(
				"a download address is not a GitHub release address".into(),
			));
		}

		let digest_ok = artifact.sha256.len() == 64
			&& artifact
				.sha256
				.bytes()
				.all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b));

		if !digest_ok {
			return Err(schema("a checksum is not 64 lowercase hex digits".into()));
		}

		if artifact.size == 0 || artifact.size > MAX_RELEASE_BYTES {
			return Err(schema("a file size is out of range".into()));
		}
	}

	Ok(())
}

#[cfg(test)]
mod tests {
	use base64::Engine;
	use base64::engine::general_purpose::STANDARD;
	use ed25519_dalek::{Signer, SigningKey};
	use serde_json::json;

	use super::*;

	const FEED: &str = "test.remade.launcher-releases";

	fn signing() -> SigningKey {
		SigningKey::from_bytes(&[9u8; 32])
	}

	fn registry(key: &SigningKey) -> KeyRegistry {
		let public = STANDARD.encode(key.verifying_key().to_bytes());

		KeyRegistry::from_test_release_fixture(&format!(
			r#"{{"schema_version":1,"feed_id":"{FEED}","test_only":true,"keys":[
				{{"key_id":"rel","algorithm":"ed25519","role":"updater","status":"active","public_key":"{public}"}}]}}"#
		))
		.unwrap()
	}

	fn release(overrides: serde_json::Value) -> serde_json::Value {
		let mut value = json!({
			"schema_version": 1, "feed_id": FEED, "channel": "stable", "revision": 5,
			"issued_at": "2030-01-10T00:00:00Z", "expires_at": "2030-02-10T00:00:00Z",
			"version": "1.2.0",
			"notes_url": "https://github.com/replicraft/remade-launcher/releases/tag/v1.2.0",
			"artifacts": [{
				"os": "linux", "arch": "x86_64", "name": "remade-1.2.0-linux-x86_64.tar.gz",
				"url": "https://github.com/replicraft/remade-launcher/releases/download/v1.2.0/remade-1.2.0-linux-x86_64.tar.gz",
				"sha256": "0".repeat(64), "size": 1000
			}]
		});

		if let (Some(base), Some(extra)) = (value.as_object_mut(), overrides.as_object()) {
			for (key, item) in extra {
				base.insert(key.clone(), item.clone());
			}
		}

		value
	}

	fn sign(key: &SigningKey, bytes: &[u8]) -> Vec<u8> {
		let signature = STANDARD.encode(key.sign(bytes).to_bytes());

		format!(
			r#"{{"version":1,"signatures":[{{"key_id":"rel","algorithm":"ed25519","signature":"{signature}"}}]}}"#
		)
		.into_bytes()
	}

	fn check(
		value: &serde_json::Value,
		high_water: Option<HighWater>,
	) -> Result<VerifiedRelease, CatalogError> {
		let key = signing();
		let bytes = serde_json::to_vec(value).unwrap();
		let registry = registry(&key);

		verify_release(
			&bytes,
			&sign(&key, &bytes),
			&ReleaseContext {
				registry: &registry,
				feed_id: FEED,
				channel: "stable",
				now: timestamp::parse("2030-01-15T00:00:00Z").unwrap(),
				high_water,
			},
		)
	}

	fn kind(result: Result<VerifiedRelease, CatalogError>) -> ErrorKind {
		result.expect_err("should be refused").kind()
	}

	#[test]
	fn a_signed_release_is_accepted() {
		let verified = check(&release(json!({})), None).unwrap();

		assert_eq!(verified.version().to_string(), "1.2.0");
		assert_eq!(verified.verified_by(), ["rel"]);
		assert!(verified.artifact_for("linux", "x86_64").is_some());
		assert!(verified.artifact_for("macos", "aarch64").is_none());
	}

	#[test]
	fn tampered_bytes_are_refused() {
		let key = signing();
		let bytes = serde_json::to_vec(&release(json!({}))).unwrap();
		let envelope = sign(&key, &bytes);
		let mut tampered = bytes.clone();
		let last = tampered.len() - 2;
		tampered[last] ^= 1;
		let registry = registry(&key);
		let result = verify_release(
			&tampered,
			&envelope,
			&ReleaseContext {
				registry: &registry,
				feed_id: FEED,
				channel: "stable",
				now: timestamp::parse("2030-01-15T00:00:00Z").unwrap(),
				high_water: None,
			},
		);

		assert_eq!(kind(result), ErrorKind::SignatureInvalid);
	}

	#[test]
	fn a_signature_from_an_untrusted_key_is_refused() {
		let other = SigningKey::from_bytes(&[3u8; 32]);
		let bytes = serde_json::to_vec(&release(json!({}))).unwrap();
		let registry = registry(&signing());
		let result = verify_release(
			&bytes,
			&sign(&other, &bytes),
			&ReleaseContext {
				registry: &registry,
				feed_id: FEED,
				channel: "stable",
				now: timestamp::parse("2030-01-15T00:00:00Z").unwrap(),
				high_water: None,
			},
		);

		assert_eq!(kind(result), ErrorKind::SignatureInvalid);
	}

	#[test]
	fn the_production_registry_has_no_keys_so_nothing_verifies() {
		let key = signing();
		let bytes = serde_json::to_vec(&release(json!({}))).unwrap();
		let registry = KeyRegistry::production_releases();
		let result = verify_release(
			&bytes,
			&sign(&key, &bytes),
			&ReleaseContext {
				registry: &registry,
				feed_id: "wiki.remade.launcher-releases",
				channel: "stable",
				now: timestamp::parse("2030-01-15T00:00:00Z").unwrap(),
				high_water: None,
			},
		);

		assert_eq!(kind(result), ErrorKind::NoTrustedSignature);
	}

	#[test]
	fn stale_future_and_foreign_documents_are_refused() {
		let expired = release(
			json!({"issued_at": "2030-01-01T00:00:00Z", "expires_at": "2030-01-10T00:00:00Z"}),
		);
		let future = release(
			json!({"issued_at": "2030-03-01T00:00:00Z", "expires_at": "2030-04-01T00:00:00Z"}),
		);
		let long = release(
			json!({"issued_at": "2030-01-01T00:00:00Z", "expires_at": "2031-01-01T00:00:00Z"}),
		);

		assert_eq!(kind(check(&expired, None)), ErrorKind::Expired);
		assert_eq!(kind(check(&future, None)), ErrorKind::FutureIssued);
		assert_eq!(kind(check(&long, None)), ErrorKind::InvalidTimeOrder);
		assert_eq!(
			kind(check(&release(json!({"feed_id": "other"})), None)),
			ErrorKind::WrongFeed
		);
		assert_eq!(
			kind(check(&release(json!({"channel": "beta"})), None)),
			ErrorKind::SchemaInvalid
		);
	}

	#[test]
	fn an_older_revision_cannot_replace_a_newer_one() {
		let known = |revision, digest| Some(HighWater { revision, digest });

		assert_eq!(
			kind(check(&release(json!({})), known(6, None))),
			ErrorKind::Rollback
		);
		assert_eq!(
			kind(check(&release(json!({})), known(5, Some([1; 32])))),
			ErrorKind::RevisionConflict
		);
		assert!(check(&release(json!({})), known(4, None)).is_ok());
	}

	#[test]
	fn unsafe_files_are_refused() {
		let with = |change: serde_json::Value| {
			let mut value = release(json!({}));
			let artifact = &mut value["artifacts"][0];

			for (key, item) in change.as_object().unwrap() {
				artifact[key] = item.clone();
			}

			value
		};

		for change in [
			json!({"url": "http://github.com/a/b/releases/download/v1/x.tar.gz"}),
			json!({"url": "https://evil.example/a/b/releases/download/v1/x.tar.gz"}),
			json!({"url": "https://github.com/a/b/archive/x.tar.gz"}),
			json!({"name": "../x.tar.gz"}),
			json!({"name": ".hidden"}),
			json!({"sha256": "ABC"}),
			json!({"sha256": "G".repeat(64)}),
			json!({"size": 0}),
			json!({"size": MAX_RELEASE_BYTES + 1}),
			json!({"os": "plan9"}),
			json!({"arch": "riscv"}),
		] {
			assert_eq!(
				kind(check(&with(change.clone()), None)),
				ErrorKind::SchemaInvalid,
				"{change}"
			);
		}
	}

	#[test]
	fn pre_releases_unknown_fields_and_duplicate_systems_are_refused() {
		assert_eq!(
			kind(check(&release(json!({"version": "1.3.0-beta.1"})), None)),
			ErrorKind::SchemaInvalid
		);
		assert_eq!(
			kind(check(&release(json!({"version": "latest"})), None)),
			ErrorKind::SchemaInvalid
		);
		assert_eq!(
			kind(check(&release(json!({"surprise": true})), None)),
			ErrorKind::SchemaInvalid
		);

		let mut twice = release(json!({}));
		let first = twice["artifacts"][0].clone();
		twice["artifacts"].as_array_mut().unwrap().push(first);

		assert_eq!(kind(check(&twice, None)), ErrorKind::SchemaInvalid);
	}
}
