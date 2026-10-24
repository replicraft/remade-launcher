//! Intake of user-supplied repository links.
//!
//! Only plain public `https://github.com/owner/repo` links are accepted, with
//! an optional `/releases/tag/<tag>` suffix. The URL is parsed, never
//! substring-matched, so look-alike hosts and embedded credentials fail.

use launcher_model::{ErrorCode, LauncherError};
use launcher_net::Url;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceRef {
	pub owner: String,
	pub repository: String,
	/// Set when the link points at one release. That selects the release for
	/// this install only; it does not enroll the app in future updates.
	pub tag: Option<String>,
}

fn rejected(why: &str) -> LauncherError {
	LauncherError::new(
		ErrorCode::InvalidSourceUrl,
		format!("That is not a supported GitHub repository link. {why}"),
	)
}

pub fn parse_source_url(input: &str) -> Result<SourceRef, LauncherError> {
	let url =
		Url::parse(input.trim()).map_err(|_| rejected("It could not be read as a web address."))?;

	if url.scheme() != "https" {
		return Err(rejected("Use an https:// link."));
	}

	if url.host_str() != Some("github.com") {
		return Err(rejected("Only github.com repositories are supported."));
	}

	if !url.username().is_empty() || url.password().is_some() || url.port().is_some() {
		return Err(rejected(
			"Links with credentials or a port are not accepted.",
		));
	}

	if url.query().is_some() || url.fragment().is_some() {
		return Err(rejected("Remove everything after the repository path."));
	}

	let segments: Vec<&str> = url
		.path_segments()
		.ok_or_else(|| rejected("The link has no path."))?
		.collect();

	// Allow one trailing slash, which shows up as an empty last segment.
	let segments = match segments.as_slice() {
		[rest @ .., ""] => rest,
		all => all,
	};

	let (owner, repository, tag) = match segments {
		[owner, repository] => (owner, repository, None),
		[owner, repository, "releases", "tag", tag] => (owner, repository, Some(*tag)),
		_ => return Err(rejected("Use a repository or release link.")),
	};

	let repository = repository.strip_suffix(".git").unwrap_or(repository);

	if !valid_owner(owner) || !valid_repository(repository) {
		return Err(rejected("The owner or repository name is not valid."));
	}

	let tag = match tag {
		Some(tag) => {
			Some(percent_decode(tag).ok_or_else(|| rejected("The release tag is malformed."))?)
		}
		None => None,
	};

	Ok(SourceRef {
		owner: (*owner).to_owned(),
		repository: repository.to_owned(),
		tag,
	})
}

/// GitHub logins: alphanumerics and single hyphens, at most 39 characters.
fn valid_owner(name: &str) -> bool {
	!name.is_empty()
		&& name.len() <= 39
		&& !name.starts_with('-')
		&& !name.ends_with('-')
		&& name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
}

/// Repository names: alphanumerics, `.`, `_`, `-`; never `.` or `..`.
fn valid_repository(name: &str) -> bool {
	!name.is_empty()
		&& name.len() <= 100
		&& name != "."
		&& name != ".."
		&& name
			.bytes()
			.all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
}

/// Decodes `%XX` escapes and rejects anything that is not valid UTF-8 or
/// that decodes to a control character or `/`.
fn percent_decode(text: &str) -> Option<String> {
	let bytes = text.as_bytes();
	let mut decoded = Vec::with_capacity(bytes.len());
	let mut index = 0;

	while index < bytes.len() {
		if bytes[index] == b'%' {
			let pair = text.get(index + 1..index + 3)?;
			decoded.push(u8::from_str_radix(pair, 16).ok()?);
			index += 3;
		} else {
			decoded.push(bytes[index]);
			index += 1;
		}
	}

	let decoded = String::from_utf8(decoded).ok()?;

	(!decoded.is_empty()
		&& decoded.len() <= 256
		&& !decoded.chars().any(|c| c.is_control() || c == '/'))
	.then_some(decoded)
}

#[cfg(test)]
mod tests {
	use super::*;

	fn parse(text: &str) -> Result<SourceRef, LauncherError> {
		parse_source_url(text)
	}

	#[test]
	fn accepts_repository_links() {
		for text in [
			"https://github.com/owner/repo",
			"https://github.com/owner/repo/",
			"https://github.com/owner/repo.git",
			"  https://github.com/owner/repo  ",
		] {
			let source = parse(text).unwrap();

			assert_eq!(
				(
					source.owner.as_str(),
					source.repository.as_str(),
					source.tag
				),
				("owner", "repo", None)
			);
		}
	}

	#[test]
	fn accepts_release_links_and_decodes_the_tag() {
		let source = parse("https://github.com/o/r/releases/tag/v1.2.3").unwrap();
		assert_eq!(source.tag.as_deref(), Some("v1.2.3"));

		let source = parse("https://github.com/o/r/releases/tag/app%2Fv1").err();
		assert!(source.is_some(), "an encoded slash must be refused");

		let source = parse("https://github.com/o/r/releases/tag/rel%20one").unwrap();
		assert_eq!(source.tag.as_deref(), Some("rel one"));
	}

	#[test]
	fn rejects_lookalikes_credentials_and_odd_shapes() {
		for text in [
			"http://github.com/o/r",
			"https://github.com.evil.example/o/r",
			"https://evilgithub.com/o/r",
			"https://www.github.com/o/r",
			"https://user:pw@github.com/o/r",
			"https://github.com:8443/o/r",
			"https://github.com/o/r?tab=readme",
			"https://github.com/o/r#install",
			"https://github.com/o",
			"https://github.com/o/r/issues/1",
			"https://github.com/o/r/releases/download/v1/a.zip",
			"https://github.com/-bad/r",
			"https://github.com/o/..",
			"ftp://github.com/o/r",
			"github.com/o/r",
			"",
			"https://github.com/o/r%00",
			"https://github.com/o/r/releases/tag/%00",
		] {
			assert!(parse(text).is_err(), "{text:?} should be rejected");
		}
	}
}
