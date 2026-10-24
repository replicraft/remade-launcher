//! Version parsing under a project's declared strategy.

use std::cmp::Ordering;

use launcher_catalog::VersionStrategy;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Version {
	Semver(semver::Version),
	/// Numeric components such as `2030.01.15`, compared left to right.
	Calver(Vec<u64>),
	/// A label with no defined ordering.
	Opaque(String),
}

/// Whether two versions can be ordered.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VersionOrder {
	/// The strategy defines an order.
	Defined,
	/// It does not. The resolver falls back to publication order and says so.
	ProviderOrder,
}

impl Version {
	/// Parses `tag`. Only a single literal `v` or `V` prefix is stripped;
	/// arbitrary leading text is never skipped.
	pub fn parse(tag: &str, strategy: VersionStrategy) -> Option<Self> {
		Self::parse_with_prefix(tag, "", strategy)
	}

	/// Like [`Self::parse`], but strips `prefix` first when it is not empty.
	/// The prefix comes from the signed tag rule that selected the release,
	/// such as `editor-v` from `editor-v*`, so monorepo tags parse without
	/// loosening the rule to "skip any leading text".
	pub fn parse_with_prefix(tag: &str, prefix: &str, strategy: VersionStrategy) -> Option<Self> {
		let bare = if prefix.is_empty() {
			tag.strip_prefix(['v', 'V']).unwrap_or(tag)
		} else {
			tag.strip_prefix(prefix)?
		};

		match strategy {
			VersionStrategy::Semver => semver::Version::parse(bare).ok().map(Self::Semver),
			VersionStrategy::Calver => parse_calver(bare).map(Self::Calver),
			VersionStrategy::Opaque => (!tag.is_empty()).then(|| Self::Opaque(tag.to_owned())),
		}
	}

	pub fn order_kind(&self) -> VersionOrder {
		match self {
			Self::Opaque(_) => VersionOrder::ProviderOrder,
			_ => VersionOrder::Defined,
		}
	}

	/// True for a semantic-version pre-release such as `1.0.0-rc.1`.
	pub fn is_prerelease(&self) -> bool {
		matches!(self, Self::Semver(version) if !version.pre.is_empty())
	}

	/// `None` when the versions are of different kinds or unordered.
	pub fn compare(&self, other: &Self) -> Option<Ordering> {
		match (self, other) {
			(Self::Semver(a), Self::Semver(b)) => Some(a.cmp(b)),
			(Self::Calver(a), Self::Calver(b)) => Some(a.cmp(b)),
			_ => None,
		}
	}
}

fn parse_calver(text: &str) -> Option<Vec<u64>> {
	let parts: Vec<&str> = text.split(['.', '-']).collect();

	if !(2..=4).contains(&parts.len()) {
		return None;
	}

	parts
		.iter()
		.map(|part| {
			(!part.is_empty() && part.len() <= 9 && part.bytes().all(|b| b.is_ascii_digit()))
				.then(|| part.parse().ok())
				.flatten()
		})
		.collect()
}

#[cfg(test)]
mod tests {
	use super::*;

	fn semver(tag: &str) -> Option<Version> {
		Version::parse(tag, VersionStrategy::Semver)
	}

	#[test]
	fn strips_only_a_literal_v() {
		assert!(semver("v1.2.3").is_some());
		assert!(semver("V1.2.3").is_some());
		assert!(semver("1.2.3").is_some());
		assert!(semver("release-1.2.3").is_none());
		assert!(semver("vv1.2.3").is_none());
	}

	#[test]
	fn a_rule_prefix_replaces_the_default_v_strip() {
		let parse = |tag: &str, prefix: &str| {
			Version::parse_with_prefix(tag, prefix, VersionStrategy::Semver)
		};

		assert!(parse("editor-v1.2.3", "editor-v").is_some());
		assert!(parse("cli-v1.2.3", "editor-v").is_none());
		assert!(parse("v1.2.3", "editor-v").is_none());
	}

	#[test]
	fn semver_orders_correctly() {
		let order = |a: &str, b: &str| semver(a).unwrap().compare(&semver(b).unwrap()).unwrap();

		assert_eq!(order("v1.10.0", "v1.9.0"), Ordering::Greater);
		assert_eq!(order("v1.0.0-rc.1", "v1.0.0"), Ordering::Less);
		assert!(semver("v1.0.0-rc.1").unwrap().is_prerelease());
		assert!(!semver("v1.0.0").unwrap().is_prerelease());
	}

	#[test]
	fn calver_compares_numerically() {
		let parse = |t: &str| Version::parse(t, VersionStrategy::Calver).unwrap();

		assert_eq!(
			parse("2030.01.15").compare(&parse("2030.1.9")),
			Some(Ordering::Greater)
		);
		assert!(Version::parse("2030", VersionStrategy::Calver).is_none());
		assert!(Version::parse("2030.x", VersionStrategy::Calver).is_none());
	}

	#[test]
	fn opaque_has_no_order() {
		let a = Version::parse("build-7", VersionStrategy::Opaque).unwrap();
		let b = Version::parse("build-8", VersionStrategy::Opaque).unwrap();

		assert_eq!(a.compare(&b), None);
		assert_eq!(a.order_kind(), VersionOrder::ProviderOrder);
	}

	#[test]
	fn mixed_kinds_do_not_compare() {
		let s = semver("1.0.0").unwrap();
		let c = Version::parse("2030.01", VersionStrategy::Calver).unwrap();

		assert_eq!(s.compare(&c), None);
	}
}
