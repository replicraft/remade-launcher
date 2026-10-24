//! The catalog's only time format: UTC RFC 3339 with whole seconds.

use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

/// Parses `YYYY-MM-DDTHH:MM:SSZ` and nothing else. Offsets, fractional
/// seconds, and lowercase letters are all rejected so that two valid
/// manifests cannot spell the same instant differently.
pub(crate) fn parse(text: &str) -> Option<OffsetDateTime> {
	let bytes = text.as_bytes();

	let shape_ok = bytes.len() == 20
		&& bytes.iter().enumerate().all(|(index, byte)| match index {
			4 | 7 => *byte == b'-',
			10 => *byte == b'T',
			13 | 16 => *byte == b':',
			19 => *byte == b'Z',
			_ => byte.is_ascii_digit(),
		});

	if !shape_ok {
		return None;
	}

	OffsetDateTime::parse(text, &Rfc3339).ok()
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn accepts_whole_second_utc() {
		assert_eq!(
			parse("2030-01-15T00:00:00Z").unwrap().unix_timestamp(),
			1_894_665_600
		);
	}

	#[test]
	fn rejects_other_spellings() {
		for text in [
			"2030-01-15T00:00:00.5Z",
			"2030-01-15T00:00:00+00:00",
			"2030-01-15t00:00:00z",
			"2030-01-15 00:00:00Z",
			"2030-13-15T00:00:00Z",
			"2030-02-30T00:00:00Z",
			"2030-01-15T24:00:00Z",
			"",
		] {
			assert!(parse(text).is_none(), "{text} should be rejected");
		}
	}
}
