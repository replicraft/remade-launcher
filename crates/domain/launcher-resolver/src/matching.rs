//! Whole-filename glob matching.
//!
//! Patterns support `*`, `?`, and `[...]` classes only. Alternation, escapes,
//! and path separators are refused, and matching is automaton-based, so a
//! hostile pattern cannot trigger catastrophic backtracking.

use globset::{GlobBuilder, GlobMatcher};

pub struct Matcher {
	matchers: Vec<GlobMatcher>,
}

impl Matcher {
	/// Returns `None` when any pattern uses unsupported syntax.
	pub fn new(patterns: &[String], case_sensitive: bool) -> Option<Self> {
		let matchers = patterns
			.iter()
			.map(|pattern| compile(pattern, case_sensitive))
			.collect::<Option<Vec<_>>>()?;

		Some(Self { matchers })
	}

	/// True when the whole `name` matches any pattern.
	pub fn is_match(&self, name: &str) -> bool {
		self.matchers.iter().any(|matcher| matcher.is_match(name))
	}
}

fn compile(pattern: &str, case_sensitive: bool) -> Option<GlobMatcher> {
	let allowed = !pattern.is_empty()
		&& !pattern.contains(['{', '}', '\\', '/', '\0'])
		&& !pattern.chars().any(char::is_control);

	if !allowed {
		return None;
	}

	GlobBuilder::new(pattern)
		.case_insensitive(!case_sensitive)
		.literal_separator(true)
		.backslash_escape(false)
		.build()
		.ok()
		.map(|glob| glob.compile_matcher())
}

#[cfg(test)]
mod tests {
	use super::*;

	fn matcher(patterns: &[&str], case_sensitive: bool) -> Matcher {
		let patterns: Vec<String> = patterns.iter().map(|p| (*p).to_owned()).collect();

		Matcher::new(&patterns, case_sensitive).unwrap()
	}

	#[test]
	fn matches_whole_names_only() {
		let m = matcher(&["app-*-linux.tar.gz"], true);

		assert!(m.is_match("app-1.2.3-linux.tar.gz"));
		assert!(!m.is_match("xapp-1.2.3-linux.tar.gz"));
		assert!(!m.is_match("app-1.2.3-linux.tar.gz.sig"));
	}

	#[test]
	fn question_mark_and_classes() {
		let m = matcher(&["tool-v?.[0-9].zip"], true);

		assert!(m.is_match("tool-v1.5.zip"));
		assert!(!m.is_match("tool-v10.5.zip"));
		assert!(!m.is_match("tool-v1.x.zip"));
	}

	#[test]
	fn case_policy_is_explicit() {
		assert!(!matcher(&["App.zip"], true).is_match("app.zip"));
		assert!(matcher(&["App.zip"], false).is_match("app.zip"));
	}

	#[test]
	fn dangerous_syntax_is_refused() {
		for pattern in ["{a,b}", "a\\b", "dir/file", "", "a\0b"] {
			assert!(
				Matcher::new(&[pattern.to_owned()], true).is_none(),
				"{pattern:?}"
			);
		}
	}

	#[test]
	fn pathological_patterns_finish_quickly() {
		let pattern = "*a*a*a*a*a*a*a*a*a*a*a*a*b".to_owned();
		let m = Matcher::new(&[pattern], true).unwrap();

		assert!(!m.is_match(&"a".repeat(2000)));
	}
}
