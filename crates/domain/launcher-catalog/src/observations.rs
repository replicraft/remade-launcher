//! The daily note of which release is newest for every catalog app.
//!
//! Checking a hundred installed apps for updates would mean a couple of GitHub
//! requests each, from every person's machine, against a limit of 60 an hour.
//! The website looks once a day instead and writes down the newest release of
//! each app. A launcher reads that one file: if the newest release is the one
//! it installed, nothing newer exists and GitHub is not asked.
//!
//! The note is only ever a hint that nothing is newer. Anything that would be
//! installed is looked up live from GitHub and checked against the signed
//! catalog, so a stale or hostile note can delay an update for the days it
//! stays valid and can never cause an install. It is signed anyway, which
//! bounds that delay: it expires, and an older one cannot replace a newer one.

use std::collections::BTreeMap;

use serde::Deserialize;
use time::{Duration, OffsetDateTime};

use crate::envelope::{self, is_identifier};
use crate::error::{CatalogError, ErrorKind};
use crate::limits::{MAX_CLOCK_SKEW_MINUTES, MAX_SAFE_INTEGER};
use crate::registry::KeyRegistry;
use crate::strict_json::{self, StrictError};
use crate::timestamp;
use crate::verify::{HighWater, verify_signatures};

/// The largest snapshot accepted, after decompression.
pub const MAX_OBSERVATIONS_BYTES: usize = 32 * 1024 * 1024;

const MAX_PROJECTS: usize = 10_000;
const MAX_ASSETS: usize = 200;
/// A snapshot is meant to be renewed daily. This is the longest it may claim.
const MAX_VALIDITY_DAYS: i64 = 14;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawObservations {
	schema_version: u32,
	/// Always `release-observations`, so no other signed document can be
	/// mistaken for this one.
	kind: String,
	feed_id: String,
	revision: u64,
	generated_at: String,
	expires_at: String,
	/// The catalog revision the snapshot was built from. Informational.
	#[serde(default)]
	catalog_revision: Option<u64>,
	projects: BTreeMap<String, ObservedProject>,
}

/// What the website saw for one catalog app.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ObservedProject {
	/// The repository the website looked at. It must be the one the catalog
	/// approved, or the note is ignored.
	pub repository_id: String,
	pub owner_id: String,
	/// The newest published release of any kind. Drafts are left out.
	pub newest: Option<ObservedRelease>,
	/// The newest release that is not a pre-release.
	pub newest_stable: Option<ObservedRelease>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ObservedRelease {
	pub id: u64,
	pub tag: String,
	pub prerelease: bool,
	#[serde(default)]
	pub published_at: Option<String>,
	/// Every file the release has. A replaced file gets a new ID, so an
	/// installed file missing from this list means the release changed.
	pub asset_ids: Vec<u64>,
}

/// A snapshot whose signature, structure, and freshness checked out.
#[derive(Debug)]
pub struct VerifiedObservations {
	revision: u64,
	catalog_revision: Option<u64>,
	generated_at: OffsetDateTime,
	expires_at: OffsetDateTime,
	digest: [u8; 32],
	projects: BTreeMap<String, ObservedProject>,
}

impl VerifiedObservations {
	pub fn revision(&self) -> u64 {
		self.revision
	}

	pub fn catalog_revision(&self) -> Option<u64> {
		self.catalog_revision
	}

	pub fn generated_at(&self) -> OffsetDateTime {
		self.generated_at
	}

	pub fn expires_at(&self) -> OffsetDateTime {
		self.expires_at
	}

	pub fn digest(&self) -> [u8; 32] {
		self.digest
	}

	/// A snapshot without a signature, for tests that need one.
	#[cfg(any(test, feature = "test-keys"))]
	pub fn for_test(projects: Vec<(String, ObservedProject)>, expires_at: OffsetDateTime) -> Self {
		Self {
			revision: 1,
			catalog_revision: None,
			generated_at: expires_at - Duration::days(1),
			expires_at,
			digest: [0; 32],
			projects: projects.into_iter().collect(),
		}
	}

	pub fn project(&self, slug: &str) -> Option<&ObservedProject> {
		self.projects.get(slug)
	}

	pub fn len(&self) -> usize {
		self.projects.len()
	}

	pub fn is_empty(&self) -> bool {
		self.projects.is_empty()
	}
}

pub struct ObservationsContext<'a> {
	/// The catalog keys. The snapshot is signed with the same key as the
	/// catalog, by the same publisher.
	pub registry: &'a KeyRegistry,
	pub feed_id: &'a str,
	pub now: OffsetDateTime,
	pub high_water: Option<HighWater>,
}

/// Checks a snapshot and its detached signatures.
pub fn verify_observations(
	manifest_bytes: &[u8],
	envelope_bytes: &[u8],
	context: &ObservationsContext<'_>,
) -> Result<VerifiedObservations, CatalogError> {
	let schema = |detail: &str| CatalogError::new(ErrorKind::SchemaInvalid, detail.to_owned());

	if manifest_bytes.len() > MAX_OBSERVATIONS_BYTES {
		return Err(CatalogError::new(
			ErrorKind::InvalidJson,
			"the release snapshot is too large",
		));
	}

	let entries = envelope::parse(envelope_bytes)?;
	let _verified_by = verify_signatures(manifest_bytes, &entries, context.registry)?;

	let value = strict_json::parse(manifest_bytes).map_err(|error| match error {
		StrictError::DuplicateKey => CatalogError::new(
			ErrorKind::DuplicateJsonKey,
			"the release snapshot repeats an object key",
		),
		StrictError::Invalid(detail) => CatalogError::new(ErrorKind::InvalidJson, detail),
	})?;
	let raw = RawObservations::deserialize(&value)
		.map_err(|error| CatalogError::new(ErrorKind::SchemaInvalid, error.to_string()))?;

	if raw.schema_version != 1 || raw.kind != "release-observations" {
		return Err(schema("this is not a release snapshot this launcher reads"));
	}

	if raw.feed_id != context.feed_id || raw.feed_id != context.registry.feed_id() {
		return Err(CatalogError::new(
			ErrorKind::WrongFeed,
			"the release snapshot is for another feed",
		));
	}

	let parse_time = |text: &str, what: &str| {
		timestamp::parse(text).ok_or_else(|| {
			CatalogError::new(
				ErrorKind::SchemaInvalid,
				format!("{what} is not a UTC whole-second timestamp"),
			)
		})
	};
	let generated_at = parse_time(&raw.generated_at, "generated_at")?;
	let expires_at = parse_time(&raw.expires_at, "expires_at")?;

	if expires_at <= generated_at || expires_at - generated_at > Duration::days(MAX_VALIDITY_DAYS) {
		return Err(CatalogError::new(
			ErrorKind::InvalidTimeOrder,
			"the validity window is empty or longer than policy allows",
		));
	}

	if generated_at > context.now + Duration::minutes(MAX_CLOCK_SKEW_MINUTES) {
		return Err(CatalogError::new(
			ErrorKind::FutureIssued,
			"the release snapshot was made in the future",
		));
	}

	if expires_at <= context.now {
		return Err(CatalogError::new(
			ErrorKind::Expired,
			"the release snapshot has expired",
		));
	}

	check_projects(&raw.projects)?;

	let digest: [u8; 32] = sha2::Sha256::digest(manifest_bytes).into();

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

	Ok(VerifiedObservations {
		revision: raw.revision,
		catalog_revision: raw.catalog_revision,
		generated_at,
		expires_at,
		digest,
		projects: raw.projects,
	})
}

fn check_projects(projects: &BTreeMap<String, ObservedProject>) -> Result<(), CatalogError> {
	let bad = |detail: String| CatalogError::new(ErrorKind::SchemaInvalid, detail);

	if projects.len() > MAX_PROJECTS {
		return Err(bad("the note lists too many apps".into()));
	}

	let id_ok = |text: &str| {
		!text.is_empty()
			&& text.len() <= 16
			&& text.bytes().all(|b| b.is_ascii_digit())
			&& text.parse::<u64>().is_ok_and(|id| id <= MAX_SAFE_INTEGER)
	};

	for (slug, project) in projects {
		if !is_identifier(slug) {
			return Err(bad(format!("{slug:?} is not a valid app name")));
		}

		if !id_ok(&project.repository_id) || !id_ok(&project.owner_id) {
			return Err(bad(format!("{slug} has an invalid repository identity")));
		}

		for release in [&project.newest, &project.newest_stable]
			.into_iter()
			.flatten()
		{
			let tag_ok = !release.tag.is_empty()
				&& release.tag.len() <= 255
				&& !release.tag.chars().any(char::is_control);
			let time_ok = release
				.published_at
				.as_deref()
				.is_none_or(|time| timestamp::parse(time).is_some());

			if release.id > MAX_SAFE_INTEGER
				|| !tag_ok || !time_ok
				|| release.asset_ids.len() > MAX_ASSETS
				|| release.asset_ids.iter().any(|id| *id > MAX_SAFE_INTEGER)
			{
				return Err(bad(format!("{slug} has an invalid release")));
			}
		}

		if project
			.newest_stable
			.as_ref()
			.is_some_and(|stable| stable.prerelease)
		{
			return Err(bad(format!(
				"{slug} lists a pre-release as its newest stable"
			)));
		}
	}

	Ok(())
}

use sha2::Digest;

#[cfg(test)]
mod tests {
	use base64::Engine;
	use base64::engine::general_purpose::STANDARD;
	use ed25519_dalek::{Signer, SigningKey};
	use serde_json::json;

	use super::*;

	const FEED: &str = "test.remade.launcher-catalog";

	fn key() -> SigningKey {
		SigningKey::from_bytes(&[5u8; 32])
	}

	fn registry() -> KeyRegistry {
		let public = STANDARD.encode(key().verifying_key().to_bytes());

		KeyRegistry::from_test_fixture(&format!(
			r#"{{"schema_version":1,"feed_id":"{FEED}","test_only":true,"keys":[
				{{"key_id":"cat","algorithm":"ed25519","role":"catalog","status":"active","public_key":"{public}"}}]}}"#
		))
		.unwrap()
	}

	fn snapshot(change: serde_json::Value) -> serde_json::Value {
		let mut value = json!({
			"schema_version": 1, "kind": "release-observations", "feed_id": FEED,
			"revision": 10, "generated_at": "2030-01-14T04:00:00Z",
			"expires_at": "2030-01-21T04:00:00Z", "catalog_revision": 3,
			"projects": {
				"sample-editor": {
					"repository_id": "101", "owner_id": "7",
					"newest": {
						"id": 9002, "tag": "v1.3.0-beta.1", "prerelease": true,
						"published_at": "2030-01-12T10:00:00Z", "asset_ids": [78]
					},
					"newest_stable": {
						"id": 9001, "tag": "v1.2.0", "prerelease": false,
						"published_at": "2030-01-10T10:00:00Z", "asset_ids": [77, 76]
					}
				}
			}
		});

		if let (Some(base), Some(extra)) = (value.as_object_mut(), change.as_object()) {
			for (name, item) in extra {
				base.insert(name.clone(), item.clone());
			}
		}

		value
	}

	fn check(
		value: &serde_json::Value,
		high_water: Option<HighWater>,
	) -> Result<VerifiedObservations, CatalogError> {
		let bytes = serde_json::to_vec(value).unwrap();
		let signature = STANDARD.encode(key().sign(&bytes).to_bytes());
		let envelope = format!(
			r#"{{"version":1,"signatures":[{{"key_id":"cat","algorithm":"ed25519","signature":"{signature}"}}]}}"#
		);

		verify_observations(
			&bytes,
			envelope.as_bytes(),
			&ObservationsContext {
				registry: &registry(),
				feed_id: FEED,
				now: timestamp::parse("2030-01-15T00:00:00Z").unwrap(),
				high_water,
			},
		)
	}

	fn kind(result: Result<VerifiedObservations, CatalogError>) -> ErrorKind {
		result.expect_err("should be refused").kind()
	}

	#[test]
	fn a_signed_snapshot_is_accepted() {
		let verified = check(&snapshot(json!({})), None).unwrap();
		let project = verified.project("sample-editor").unwrap();

		assert_eq!(verified.len(), 1);
		assert_eq!(project.newest_stable.as_ref().unwrap().tag, "v1.2.0");
		assert_eq!(project.newest.as_ref().unwrap().asset_ids, vec![78]);
		assert!(verified.project("other").is_none());
	}

	#[test]
	fn a_catalog_or_release_document_is_not_a_snapshot() {
		assert_eq!(
			kind(check(&snapshot(json!({"kind": "catalog"})), None)),
			ErrorKind::SchemaInvalid
		);
		assert_eq!(
			kind(check(&snapshot(json!({"feed_id": "other"})), None)),
			ErrorKind::WrongFeed
		);
		assert_eq!(
			kind(check(&snapshot(json!({"surprise": 1})), None)),
			ErrorKind::SchemaInvalid
		);
	}

	#[test]
	fn a_stale_future_or_endless_snapshot_is_refused() {
		let old = snapshot(
			json!({"generated_at": "2030-01-01T00:00:00Z", "expires_at": "2030-01-08T00:00:00Z"}),
		);
		let future = snapshot(
			json!({"generated_at": "2030-02-01T00:00:00Z", "expires_at": "2030-02-08T00:00:00Z"}),
		);
		let long = snapshot(
			json!({"generated_at": "2030-01-01T00:00:00Z", "expires_at": "2030-03-01T00:00:00Z"}),
		);

		assert_eq!(kind(check(&old, None)), ErrorKind::Expired);
		assert_eq!(kind(check(&future, None)), ErrorKind::FutureIssued);
		assert_eq!(kind(check(&long, None)), ErrorKind::InvalidTimeOrder);
	}

	#[test]
	fn an_older_snapshot_cannot_replace_a_newer_one() {
		let known = |revision, digest| Some(HighWater { revision, digest });

		assert_eq!(
			kind(check(&snapshot(json!({})), known(11, None))),
			ErrorKind::Rollback
		);
		assert_eq!(
			kind(check(&snapshot(json!({})), known(10, Some([1; 32])))),
			ErrorKind::RevisionConflict
		);
		assert!(check(&snapshot(json!({})), known(9, None)).is_ok());
	}

	#[test]
	fn malformed_entries_are_refused_whole() {
		let mut bad_id = snapshot(json!({}));

		bad_id["projects"]["sample-editor"]["repository_id"] = json!("12ab");

		let mut bad_time = snapshot(json!({}));

		bad_time["projects"]["sample-editor"]["newest_stable"]["published_at"] = json!("yesterday");

		let mut bad_tag = snapshot(json!({}));

		bad_tag["projects"]["sample-editor"]["newest"]["tag"] = json!("");

		let mut stable_is_pre = snapshot(json!({}));

		stable_is_pre["projects"]["sample-editor"]["newest_stable"]["prerelease"] = json!(true);

		let mut extra = snapshot(json!({}));

		extra["projects"]["sample-editor"]["surprise"] = json!(1);

		let mut bad_slug = snapshot(json!({}));
		let project = bad_slug["projects"]["sample-editor"].clone();

		bad_slug["projects"] = json!({"../up": project});

		for value in [bad_id, bad_time, bad_tag, stable_is_pre, extra, bad_slug] {
			assert_eq!(kind(check(&value, None)), ErrorKind::SchemaInvalid);
		}
	}

	#[test]
	fn an_app_with_no_stable_release_is_allowed() {
		let mut value = snapshot(json!({}));

		value["projects"]["sample-editor"]["newest_stable"] = json!(null);

		assert!(
			check(&value, None)
				.unwrap()
				.project("sample-editor")
				.unwrap()
				.newest_stable
				.is_none()
		);
	}

	#[test]
	fn a_tampered_snapshot_fails_its_signature() {
		let value = snapshot(json!({}));
		let bytes = serde_json::to_vec(&value).unwrap();
		let signature = STANDARD.encode(key().sign(&bytes).to_bytes());
		let envelope = format!(
			r#"{{"version":1,"signatures":[{{"key_id":"cat","algorithm":"ed25519","signature":"{signature}"}}]}}"#
		);
		let mut tampered = bytes.clone();
		let at = tampered.len() - 5;

		tampered[at] ^= 1;

		let error = verify_observations(
			&tampered,
			envelope.as_bytes(),
			&ObservationsContext {
				registry: &registry(),
				feed_id: FEED,
				now: timestamp::parse("2030-01-15T00:00:00Z").unwrap(),
				high_water: None,
			},
		)
		.unwrap_err();

		assert_eq!(error.kind(), ErrorKind::SignatureInvalid);
	}
}
