//! Archive path rules.
//!
//! The same rules apply on every host, so an archive that is safe on Linux is
//! also safe to install on Windows, and one platform's leniency never becomes
//! another's hole.

use crate::error::PackageError;

const RESERVED: &[&str] = &[
	"con", "prn", "aux", "nul", "com1", "com2", "com3", "com4", "com5", "com6", "com7", "com8",
	"com9", "lpt1", "lpt2", "lpt3", "lpt4", "lpt5", "lpt6", "lpt7", "lpt8", "lpt9",
];

fn unsafe_path(raw: &str, why: &str) -> PackageError {
	PackageError::UnsafePath(format!("{raw:?} ({why})"))
}

/// Normalizes an entry path and drops `strip` leading components.
///
/// Returns `None` for an entry that disappears after stripping, such as the
/// top-level directory itself.
pub(crate) fn normalize(raw: &str, strip: u8) -> Result<Option<String>, PackageError> {
	let trimmed = raw.strip_suffix('/').unwrap_or(raw);
	// Tools that archive `./dir` produce a harmless leading `./`.
	let trimmed = trimmed.strip_prefix("./").unwrap_or(trimmed);

	if trimmed.is_empty() || trimmed == "." {
		return Ok(None);
	}

	if trimmed.starts_with('/') || trimmed.contains('\\') {
		return Err(unsafe_path(raw, "absolute or backslash path"));
	}

	if trimmed.len() > 1024 {
		return Err(unsafe_path(raw, "too long"));
	}

	let components: Vec<&str> = trimmed.split('/').collect();

	for component in &components {
		check_component(raw, component)?;
	}

	let strip = usize::from(strip);

	if components.len() <= strip {
		return Ok(None);
	}

	Ok(Some(components[strip..].join("/")))
}

fn check_component(raw: &str, component: &str) -> Result<(), PackageError> {
	if component.is_empty() || component == "." || component == ".." {
		return Err(unsafe_path(raw, "empty, dot, or dot-dot component"));
	}

	if component.len() > 255 {
		return Err(unsafe_path(raw, "component too long"));
	}

	if component
		.chars()
		.any(|c| c.is_control() || matches!(c, ':' | '<' | '>' | '"' | '|' | '?' | '*'))
	{
		return Err(unsafe_path(raw, "reserved or control character"));
	}

	if component.ends_with('.') || component.ends_with(' ') {
		return Err(unsafe_path(raw, "trailing dot or space"));
	}

	let stem = component
		.split('.')
		.next()
		.unwrap_or(component)
		.to_ascii_lowercase();

	if RESERVED.contains(&stem.as_str()) {
		return Err(unsafe_path(raw, "reserved device name"));
	}

	Ok(())
}

/// Checks that a symlink at `entry_path` pointing to `target` stays inside
/// the install root, judged lexically.
pub(crate) fn symlink_target_is_contained(entry_path: &str, target: &str) -> bool {
	if target.is_empty() || target.starts_with('/') || target.contains('\\') || target.contains(':')
	{
		return false;
	}

	if target.chars().any(char::is_control) {
		return false;
	}

	let mut depth = entry_path.split('/').count().saturating_sub(1) as i64;

	for part in target.split('/') {
		match part {
			"" | "." => {}
			".." => {
				depth -= 1;

				if depth < 0 {
					return false;
				}
			}
			_ => depth += 1,
		}
	}

	true
}

#[cfg(test)]
mod tests {
	use super::*;

	fn ok(raw: &str, strip: u8) -> Option<String> {
		normalize(raw, strip).unwrap()
	}

	#[test]
	fn keeps_ordinary_paths_and_strips_leading_components() {
		assert_eq!(ok("bin/app", 0).as_deref(), Some("bin/app"));
		assert_eq!(ok("app-1.0/bin/app", 1).as_deref(), Some("bin/app"));
		assert_eq!(ok("./app-1.0/bin/app", 1).as_deref(), Some("bin/app"));
		assert_eq!(ok("app-1.0/", 1), None);
		assert_eq!(ok("./", 0), None);
	}

	#[test]
	fn unsafe_paths_are_rejected_even_when_stripped_away() {
		for raw in [
			"/etc/passwd",
			"../escape",
			"a/../../escape",
			"a/./b",
			"a//b",
			"a\\b",
			"C:/windows",
			"c:evil",
			"a/b\0c",
			"a/\nb",
			"CON",
			"a/nul.txt",
			"a/LPT1",
			"trailing.",
			"space /x",
			"quo\"te",
		] {
			assert!(normalize(raw, 0).is_err(), "{raw:?}");
		}

		// The unsafe component is in the part that would be stripped.
		assert!(normalize("../x/y", 1).is_err());
	}

	#[test]
	fn symlinks_must_stay_inside() {
		assert!(symlink_target_is_contained("lib/libx.so", "libx.so.1"));
		assert!(symlink_target_is_contained("a/b/link", "../c"));
		assert!(!symlink_target_is_contained("a/link", "../../outside"));
		assert!(!symlink_target_is_contained("link", "../x"));
		assert!(!symlink_target_is_contained("link", "/etc/passwd"));
		assert!(!symlink_target_is_contained("link", "C:\\x"));
		assert!(!symlink_target_is_contained("link", ""));
	}
}
