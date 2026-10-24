//! The detached signature envelope.

use std::collections::HashSet;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use ed25519_dalek::Signature;
use serde::Deserialize;

use crate::error::{CatalogError, ErrorKind};
use crate::limits::{MAX_ENVELOPE_BYTES, MAX_SIGNATURES};
use crate::strict_json::{self, StrictError};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawEnvelope {
	version: u32,
	signatures: Vec<RawSignature>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawSignature {
	key_id: String,
	algorithm: String,
	signature: String,
}

/// One entry of a validated envelope.
pub(crate) struct SignatureEntry {
	pub(crate) key_id: String,
	pub(crate) signature: Signature,
}

/// Parses and validates the envelope. A malformed envelope is rejected as a
/// whole: one good signature does not rescue a broken one beside it.
pub(crate) fn parse(bytes: &[u8]) -> Result<Vec<SignatureEntry>, CatalogError> {
	let invalid = |detail: String| CatalogError::new(ErrorKind::EnvelopeInvalid, detail);

	if bytes.len() > MAX_ENVELOPE_BYTES {
		return Err(invalid("the envelope is too large".into()));
	}

	let value = strict_json::parse(bytes).map_err(|error| match error {
		StrictError::DuplicateKey => invalid("the envelope has a duplicate key".into()),
		StrictError::Invalid(detail) => invalid(detail),
	})?;

	let raw = RawEnvelope::deserialize(&value).map_err(|error| invalid(error.to_string()))?;

	if raw.version != 1 {
		return Err(invalid(format!(
			"unsupported envelope version {}",
			raw.version
		)));
	}

	if raw.signatures.is_empty() || raw.signatures.len() > MAX_SIGNATURES {
		return Err(invalid(
			"the envelope has an unsupported number of signatures".into(),
		));
	}

	let mut seen = HashSet::new();
	let mut entries = Vec::with_capacity(raw.signatures.len());

	for raw_signature in raw.signatures {
		if !is_identifier(&raw_signature.key_id) {
			return Err(invalid("a key ID is malformed".into()));
		}

		if raw_signature.algorithm != "ed25519" {
			return Err(invalid("a signature uses an unsupported algorithm".into()));
		}

		if !seen.insert(raw_signature.key_id.clone()) {
			return Err(CatalogError::new(
				ErrorKind::DuplicateKeyId,
				"two signatures claim the same key ID",
			));
		}

		let decoded = STANDARD
			.decode(&raw_signature.signature)
			.map_err(|_| invalid("a signature is not valid Base64".into()))?;
		let signature = Signature::from_slice(&decoded)
			.map_err(|_| invalid("a signature has the wrong length".into()))?;

		entries.push(SignatureEntry {
			key_id: raw_signature.key_id,
			signature,
		});
	}

	Ok(entries)
}

/// `^[a-zA-Z0-9][a-zA-Z0-9._-]*$`, at most 128 characters.
pub(crate) fn is_identifier(text: &str) -> bool {
	let mut characters = text.chars();

	text.len() <= 128
		&& characters
			.next()
			.is_some_and(|first| first.is_ascii_alphanumeric())
		&& characters.all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
}

#[cfg(test)]
mod tests {
	use super::*;

	const SIGNATURE: &str =
		"XIIxgPcyZSeOUF871dO30IpxDbqFsy1Aebe3nJs24SEFsMMqwwyqwcIdLXfVLcLJ+XlZxYlZra7Axo28L/ooAg==";

	fn envelope(signatures: &str) -> String {
		format!(r#"{{"version":1,"signatures":[{signatures}]}}"#)
	}

	fn entry(key_id: &str, algorithm: &str, signature: &str) -> String {
		format!(r#"{{"key_id":"{key_id}","algorithm":"{algorithm}","signature":"{signature}"}}"#)
	}

	#[test]
	fn accepts_a_wellformed_envelope() {
		let parsed = parse(envelope(&entry("a", "ed25519", SIGNATURE)).as_bytes()).unwrap();

		assert_eq!(parsed.len(), 1);
		assert_eq!(parsed[0].key_id, "a");
	}

	#[test]
	fn rejects_duplicate_key_ids() {
		let both = format!(
			"{},{}",
			entry("a", "ed25519", SIGNATURE),
			entry("a", "ed25519", SIGNATURE)
		);
		let error = parse(envelope(&both).as_bytes()).err().unwrap();

		assert_eq!(error.kind(), ErrorKind::DuplicateKeyId);
	}

	#[test]
	fn rejects_unknown_algorithms_and_bad_encodings() {
		for bad in [
			entry("a", "rsa", SIGNATURE),
			entry("a", "ed25519", "not base64!"),
			entry("a", "ed25519", "AAAA"),
			entry("-a", "ed25519", SIGNATURE),
		] {
			let error = parse(envelope(&bad).as_bytes()).err().unwrap();

			assert_eq!(error.kind(), ErrorKind::EnvelopeInvalid);
		}
	}

	#[test]
	fn rejects_unsupported_shapes() {
		for text in [
			r#"{"version":2,"signatures":[]}"#.to_owned(),
			r#"{"version":1,"signatures":[]}"#.to_owned(),
			r#"{"version":1}"#.to_owned(),
			format!(
				r#"{{"version":1,"signatures":[{}],"extra":1}}"#,
				entry("a", "ed25519", SIGNATURE)
			),
			"[]".to_owned(),
		] {
			assert_eq!(
				parse(text.as_bytes()).err().unwrap().kind(),
				ErrorKind::EnvelopeInvalid
			);
		}
	}

	#[test]
	fn rejects_oversized_envelopes() {
		let huge = vec![b' '; MAX_ENVELOPE_BYTES + 1];

		assert_eq!(
			parse(&huge).err().unwrap().kind(),
			ErrorKind::EnvelopeInvalid
		);
	}
}
