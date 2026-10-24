//! The set of catalog public keys this build trusts.
//!
//! The production registry is compiled in from a reviewed file. It is never
//! fetched, and the website cannot change it. Test fixtures use a different
//! feed ID and a `test_only` flag, and the production loader refuses any
//! document carrying that flag.

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use ed25519_dalek::VerifyingKey;
use serde::Deserialize;

use crate::envelope::is_identifier;

const PRODUCTION_REGISTRY: &str = include_str!("../registry/production.json");
const RELEASE_REGISTRY: &str = include_str!("../registry/releases.json");

/// What a key may sign. A catalog key cannot sign a launcher release and the
/// other way round, so losing one signing key never reaches the other.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Role {
	Catalog,
	Updater,
}

impl Role {
	const fn as_str(self) -> &'static str {
		match self {
			Self::Catalog => "catalog",
			Self::Updater => "updater",
		}
	}
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RegistryDocument {
	schema_version: u32,
	feed_id: String,
	#[serde(default)]
	test_only: bool,
	keys: Vec<KeyDocument>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct KeyDocument {
	key_id: String,
	algorithm: String,
	#[serde(default = "catalog_role")]
	role: String,
	status: Status,
	public_key: String,
}

fn catalog_role() -> String {
	"catalog".to_owned()
}

#[derive(Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
enum Status {
	Active,
	Revoked,
}

pub(crate) struct TrustedKey {
	pub(crate) key_id: String,
	pub(crate) key: VerifyingKey,
}

/// Locally trusted catalog keys for one feed.
pub struct KeyRegistry {
	feed_id: String,
	active: Vec<TrustedKey>,
}

impl KeyRegistry {
	/// The registry compiled into official builds.
	///
	/// It currently lists no keys, because the owner has not yet published
	/// the production catalog public keys. Until they are added, every
	/// network catalog fails with `no-trusted-signature`.
	pub fn production() -> Self {
		Self::load(PRODUCTION_REGISTRY, false, Role::Catalog)
			.expect("the compiled-in production registry is valid, which a unit test checks")
	}

	/// The keys that may sign launcher releases, compiled in like the catalog
	/// keys. It lists none until the owner publishes a release signing key, so
	/// until then no launcher update is ever accepted.
	pub fn production_releases() -> Self {
		Self::load(RELEASE_REGISTRY, false, Role::Updater)
			.expect("the compiled-in release registry is valid, which a unit test checks")
	}

	/// A registry of test-only keys. Compiled out of builds that do not
	/// enable the `test-keys` feature.
	#[cfg(any(test, feature = "test-keys"))]
	pub fn from_test_fixture(json: &str) -> Result<Self, String> {
		Self::load(json, true, Role::Catalog)
	}

	/// The same for launcher release keys.
	#[cfg(any(test, feature = "test-keys"))]
	pub fn from_test_release_fixture(json: &str) -> Result<Self, String> {
		Self::load(json, true, Role::Updater)
	}

	pub fn feed_id(&self) -> &str {
		&self.feed_id
	}

	/// The IDs of the keys this build trusts. Public by nature: a key ID
	/// names a key, it does not grant anything.
	pub fn active_key_ids(&self) -> Vec<&str> {
		self.active.iter().map(|key| key.key_id.as_str()).collect()
	}

	pub fn active_key_count(&self) -> usize {
		self.active.len()
	}

	pub(crate) fn active_key(&self, key_id: &str) -> Option<&TrustedKey> {
		self.active.iter().find(|key| key.key_id == key_id)
	}

	fn load(json: &str, expect_test_only: bool, role: Role) -> Result<Self, String> {
		let document: RegistryDocument =
			serde_json::from_str(json).map_err(|error| error.to_string())?;

		if document.schema_version != 1 {
			return Err("unsupported registry schema".into());
		}

		if document.test_only != expect_test_only {
			return Err("test-only keys and production keys cannot be mixed".into());
		}

		let mut active = Vec::new();

		for key in document.keys {
			if !is_identifier(&key.key_id)
				|| key.algorithm != "ed25519"
				|| key.role != role.as_str()
			{
				return Err(format!(
					"key {} is not a valid {} key",
					key.key_id,
					role.as_str()
				));
			}

			if active
				.iter()
				.any(|existing: &TrustedKey| existing.key_id == key.key_id)
			{
				return Err(format!("key {} is listed twice", key.key_id));
			}

			if key.status != Status::Active {
				continue;
			}

			let bytes: [u8; 32] = STANDARD
				.decode(&key.public_key)
				.ok()
				.and_then(|bytes| bytes.try_into().ok())
				.ok_or_else(|| format!("key {} has a malformed public key", key.key_id))?;
			let verifying = VerifyingKey::from_bytes(&bytes)
				.map_err(|_| format!("key {} is not a valid Ed25519 point", key.key_id))?;

			active.push(TrustedKey {
				key_id: key.key_id,
				key: verifying,
			});
		}

		Ok(Self {
			feed_id: document.feed_id,
			active,
		})
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	const TEST_REGISTRY: &str = include_str!("../tests/fixtures/test-public-key-registry.json");

	#[test]
	fn the_production_registry_loads_and_is_not_a_test_registry() {
		let registry = KeyRegistry::production();

		assert_eq!(registry.feed_id(), "wiki.remade.launcher-catalog");
		assert!(!PRODUCTION_REGISTRY.contains("test_only"));
	}

	#[test]
	fn production_refuses_test_only_keys() {
		assert!(KeyRegistry::load(TEST_REGISTRY, false, Role::Catalog).is_err());
	}

	#[test]
	fn the_fixture_registry_loads_as_test_only() {
		let registry = KeyRegistry::from_test_fixture(TEST_REGISTRY).unwrap();

		assert_eq!(registry.feed_id(), "test.remade.launcher-catalog");
		assert_eq!(registry.active_key_count(), 2);
		assert!(registry.active_key("test-catalog-a").is_some());
	}

	#[test]
	fn test_loader_refuses_production_documents() {
		assert!(KeyRegistry::from_test_fixture(PRODUCTION_REGISTRY).is_err());
	}

	#[test]
	fn revoked_keys_are_not_trusted() {
		let json = r#"{"schema_version":1,"feed_id":"f","test_only":true,"keys":[
			{"key_id":"old","algorithm":"ed25519","role":"catalog","status":"revoked",
			 "public_key":"1gcMX2LHyblding3+M4Rv+pwwzkoRDxsZM/PG4WMVhc="}]}"#;

		assert_eq!(
			KeyRegistry::from_test_fixture(json)
				.unwrap()
				.active_key_count(),
			0
		);
	}

	#[test]
	fn non_catalog_roles_are_refused() {
		let json = r#"{"schema_version":1,"feed_id":"f","test_only":true,"keys":[
			{"key_id":"u","algorithm":"ed25519","role":"updater","status":"active",
			 "public_key":"1gcMX2LHyblding3+M4Rv+pwwzkoRDxsZM/PG4WMVhc="}]}"#;

		assert!(KeyRegistry::from_test_fixture(json).is_err());
	}

	#[test]
	fn the_release_registry_loads_and_lists_no_keys_yet() {
		let registry = KeyRegistry::production_releases();

		assert_eq!(registry.feed_id(), "wiki.remade.launcher-releases");
		assert_eq!(registry.active_key_count(), 0);
	}

	#[test]
	fn catalog_keys_cannot_sign_releases_or_the_other_way_round() {
		let key = |role: &str| {
			format!(
				r#"{{"schema_version":1,"feed_id":"f","test_only":true,"keys":[
				{{"key_id":"k","algorithm":"ed25519","role":"{role}","status":"active",
				 "public_key":"1gcMX2LHyblding3+M4Rv+pwwzkoRDxsZM/PG4WMVhc="}}]}}"#
			)
		};

		assert!(KeyRegistry::from_test_release_fixture(&key("updater")).is_ok());
		assert!(KeyRegistry::from_test_release_fixture(&key("catalog")).is_err());
		assert!(KeyRegistry::from_test_fixture(&key("updater")).is_err());
	}
}
