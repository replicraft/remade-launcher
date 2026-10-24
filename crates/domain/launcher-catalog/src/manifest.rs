//! The typed shape of the catalog manifest.
//!
//! Every object that carries policy rejects unknown fields, so a newer
//! publisher cannot slip in a field this build would silently ignore. Only
//! `presentation_extensions` is open, and it holds inert text.
//!
//! These types are plain data. Constructing one does not make it trusted:
//! only [`crate::VerifiedCatalog`] carries that guarantee.

use std::collections::BTreeMap;

use launcher_model::{Abi, Arch, Os};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
	pub schema_version: u32,
	pub feed_id: String,
	pub revision: u64,
	pub issued_at: String,
	pub expires_at: String,
	pub required_capabilities: Vec<String>,
	pub projects: BTreeMap<String, Project>,
	pub hash: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Project {
	pub display: Display,
	pub source: Source,
	pub status: ProjectStatus,
	pub release_policy: ReleasePolicy,
	pub variants: Vec<Variant>,
	#[serde(default)]
	pub detection: Option<Detection>,
	#[serde(default)]
	pub presentation_extensions: BTreeMap<String, String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Display {
	pub name: String,
	pub summary: String,
	pub kind: ProjectKind,
	pub categories: Vec<String>,
	pub links: Links,
	#[serde(default)]
	pub license: Option<String>,
	#[serde(default)]
	pub icon_url: Option<String>,
	#[serde(default)]
	pub screenshots: Vec<String>,
	/// The catalog's editors chose to highlight this app.
	#[serde(default)]
	pub featured: bool,
	/// When the app joined the catalog, as a UTC whole-second timestamp.
	#[serde(default)]
	pub added_at: Option<String>,
}

impl Display {
	/// [`Self::added_at`] as Unix seconds. Verification has already rejected
	/// anything that does not parse.
	pub fn added_at_unix(&self) -> Option<i64> {
		self.added_at
			.as_deref()
			.and_then(crate::timestamp::parse)
			.map(|time| time.unix_timestamp())
	}
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProjectKind {
	Desktop,
	Cli,
	Service,
	Launcher,
}

impl ProjectKind {
	pub const fn label(self) -> &'static str {
		match self {
			Self::Desktop => "Desktop app",
			Self::Cli => "Command-line tool",
			Self::Service => "Service",
			Self::Launcher => "Launcher",
		}
	}
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Links {
	pub homepage: Option<String>,
	pub documentation: Option<String>,
	pub source: Option<String>,
	pub changelog: Option<String>,
	pub community: Option<String>,
	pub license: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Source {
	pub provider: Provider,
	pub repository_id: String,
	pub owner_id: String,
	pub owner: String,
	pub repository: String,
	#[serde(default)]
	pub product_filter: Option<String>,
}

impl Source {
	/// `owner/repository`, shown next to any install action.
	pub fn display_name(&self) -> String {
		format!("{}/{}", self.owner, self.repository)
	}
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Provider {
	Github,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectStatus {
	pub state: State,
	#[serde(default)]
	pub reason: Option<String>,
	#[serde(default)]
	pub advisory_url: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum State {
	Active,
	Paused,
	Revoked,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReleasePolicy {
	pub channel: Channel,
	pub version_strategy: VersionStrategy,
	pub tag_include: Vec<String>,
	pub tag_exclude: Vec<String>,
	pub tracking: Tracking,
	#[serde(default)]
	pub pinned_release_id: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Channel {
	Stable,
	Beta,
	Nightly,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VersionStrategy {
	Semver,
	Calver,
	Opaque,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Tracking {
	Upstream,
	Pinned,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Variant {
	pub id: String,
	pub os: Os,
	pub arch: Arch,
	pub abi: Abi,
	#[serde(default)]
	pub minimum_os: Option<String>,
	pub runtimes: Vec<Runtime>,
	pub format: PackageFormat,
	pub assets: AssetSelector,
	pub install: Install,
	pub verification: Verification,
	pub required_capabilities: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Runtime {
	pub name: String,
	pub minimum_version: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
pub enum PackageFormat {
	#[serde(rename = "zip")]
	Zip,
	#[serde(rename = "tar.gz")]
	TarGz,
	#[serde(rename = "tar.xz")]
	TarXz,
	#[serde(rename = "tar.zst")]
	TarZst,
	#[serde(rename = "7z")]
	SevenZip,
	#[serde(rename = "appimage")]
	AppImage,
	#[serde(rename = "dmg")]
	Dmg,
	#[serde(rename = "app-bundle-zip")]
	AppBundleZip,
	#[serde(rename = "msi")]
	Msi,
	#[serde(rename = "exe-installer")]
	ExeInstaller,
	#[serde(rename = "deb")]
	Deb,
	#[serde(rename = "rpm")]
	Rpm,
	#[serde(rename = "pkg")]
	Pkg,
	#[serde(rename = "pkg-macos")]
	PkgMacos,
}

impl PackageFormat {
	pub const fn label(self) -> &'static str {
		match self {
			Self::Zip => "ZIP",
			Self::TarGz => "tar.gz",
			Self::TarXz => "tar.xz",
			Self::TarZst => "tar.zst",
			Self::SevenZip => "7z",
			Self::AppImage => "AppImage",
			Self::Dmg => "DMG",
			Self::AppBundleZip => "App bundle (ZIP)",
			Self::Msi => "MSI",
			Self::ExeInstaller => "EXE installer",
			Self::Deb => "DEB",
			Self::Rpm => "RPM",
			Self::Pkg => "PKG",
			Self::PkgMacos => "macOS PKG",
		}
	}
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssetSelector {
	pub include: Vec<String>,
	pub exclude: Vec<String>,
	pub case_sensitive: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Install {
	pub adapter: String,
	pub scope: InstallScope,
	#[serde(default)]
	pub entry_point: Option<String>,
	#[serde(default)]
	pub bundle_locator: Option<String>,
	#[serde(default)]
	pub expected_identity: Option<ExpectedIdentity>,
	#[serde(default)]
	pub strip_components: Option<u8>,
	pub required_capabilities: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
pub enum InstallScope {
	#[serde(rename = "user")]
	User,
	#[serde(rename = "system")]
	System,
	#[serde(rename = "user-or-system")]
	UserOrSystem,
	#[serde(rename = "external")]
	External,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExpectedIdentity {
	pub kind: IdentityKind,
	pub value: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
pub enum IdentityKind {
	#[serde(rename = "bundle-id")]
	BundleId,
	#[serde(rename = "msi-product-code")]
	MsiProductCode,
	#[serde(rename = "package-id")]
	PackageId,
	#[serde(rename = "portable-app-id")]
	PortableAppId,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Verification {
	pub minimum: VerificationLevel,
	#[serde(default)]
	pub sha256: Option<String>,
	#[serde(default)]
	pub publisher_policy: Option<PublisherPolicy>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
pub enum VerificationLevel {
	#[serde(rename = "host-digest")]
	HostDigest,
	#[serde(rename = "pinned-sha256")]
	PinnedSha256,
	#[serde(rename = "publisher-signature")]
	PublisherSignature,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublisherPolicy {
	pub format: PublisherFormat,
	pub identity: String,
	#[serde(default)]
	pub public_key_base64: Option<String>,
	#[serde(default)]
	pub signature_assets: Option<SignatureAssets>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
pub enum PublisherFormat {
	#[serde(rename = "authenticode")]
	Authenticode,
	#[serde(rename = "apple-code-signature")]
	AppleCodeSignature,
	#[serde(rename = "ed25519-detached")]
	Ed25519Detached,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignatureAssets {
	pub include: Vec<String>,
	pub exclude: Vec<String>,
	pub case_sensitive: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Detection {
	pub identities: Vec<ExpectedIdentity>,
	pub standard_locations_only: bool,
}

#[cfg(test)]
mod tests {
	use super::*;

	fn display(extra: &str) -> Display {
		let json = format!(
			r#"{{"name":"A","summary":"s","kind":"desktop","categories":[],"links":{{}}{extra}}}"#
		);

		serde_json::from_str(&json).expect("a valid display")
	}

	#[test]
	fn featured_and_added_at_are_optional() {
		let plain = display("");

		assert!(!plain.featured);
		assert_eq!(plain.added_at_unix(), None);
	}

	#[test]
	fn added_at_reads_as_unix_seconds() {
		let shown = display(r#","featured":true,"added_at":"2030-01-15T00:00:00Z""#);

		assert!(shown.featured);
		assert_eq!(shown.added_at_unix(), Some(1_894_665_600));
	}

	#[test]
	fn a_malformed_added_at_has_no_value() {
		assert_eq!(display(r#","added_at":"yesterday""#).added_at_unix(), None);
	}
}
