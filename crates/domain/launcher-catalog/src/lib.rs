//! Verification of the signed Remade catalog.
//!
//! Untrusted bytes go in; a [`VerifiedCatalog`] comes out, or a typed
//! [`CatalogError`]. There is no other way to build a `VerifiedCatalog`, so
//! any code holding one knows the bytes passed every check in the pipeline:
//!
//! 1. the envelope is well formed and bounded,
//! 2. at least one locally trusted Ed25519 key signed the exact bytes,
//! 3. the manifest parses strictly (no duplicate keys, no unknown fields),
//! 4. the feed, schema, time window, capabilities, and policy are acceptable,
//! 5. the canonical hash of `projects` matches, and
//! 6. the revision does not roll back or conflict with what we accepted.

mod envelope;
mod error;
mod manifest;
mod registry;
mod semantic;
mod strict_json;
mod timestamp;
mod verified;
mod verify;

pub mod limits;

#[cfg(test)]
mod conformance;

pub use error::{CatalogError, ErrorKind};
pub use manifest::{
	AssetSelector, Channel, Detection, Display, ExpectedIdentity, IdentityKind, Install,
	InstallScope, Links, Manifest, PackageFormat, Project, ProjectKind, ProjectStatus, Provider,
	PublisherFormat, PublisherPolicy, ReleasePolicy, Runtime, SignatureAssets, Source, State,
	Tracking, Variant, Verification, VerificationLevel, VersionStrategy,
};
pub use registry::KeyRegistry;
pub use verified::{Freshness, VerifiedCatalog};
pub use verify::{
	Change, FreshnessPolicy, HighWater, Verification as VerifyOutcome, VerifyContext, verify,
};

/// Capabilities this build can execute. Variants that need anything else are
/// shown as unavailable rather than half-supported.
///
/// `safe-archive-v1` is the bounded zip and tar.gz inspection and extraction
/// in `launcher-packages`.
pub const SUPPORTED_CAPABILITIES: &[&str] = &["safe-archive-v1"];
