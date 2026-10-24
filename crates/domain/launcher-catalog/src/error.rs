use thiserror::Error;

/// A stable label for each way verification can fail. These match the
/// `expected` values in the shared conformance fixtures.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ErrorKind {
	EnvelopeInvalid,
	DuplicateKeyId,
	NoTrustedSignature,
	SignatureInvalid,
	InvalidJson,
	DuplicateJsonKey,
	SchemaInvalid,
	WrongFeed,
	InvalidTimeOrder,
	FutureIssued,
	Expired,
	UnsupportedCapability,
	DuplicateVariant,
	HashMismatch,
	Rollback,
	RevisionConflict,
}

impl ErrorKind {
	pub const fn as_str(self) -> &'static str {
		match self {
			Self::EnvelopeInvalid => "envelope-invalid",
			Self::DuplicateKeyId => "duplicate-key-id",
			Self::NoTrustedSignature => "no-trusted-signature",
			Self::SignatureInvalid => "signature-invalid",
			Self::InvalidJson => "invalid-json",
			Self::DuplicateJsonKey => "duplicate-json-key",
			Self::SchemaInvalid => "schema-invalid",
			Self::WrongFeed => "wrong-feed",
			Self::InvalidTimeOrder => "invalid-time-order",
			Self::FutureIssued => "future-issued",
			Self::Expired => "expired",
			Self::UnsupportedCapability => "unsupported-capability",
			Self::DuplicateVariant => "duplicate-variant",
			Self::HashMismatch => "hash-mismatch",
			Self::Rollback => "rollback",
			Self::RevisionConflict => "revision-conflict",
		}
	}

	/// Failures that mean the bytes may have been tampered with, as opposed
	/// to a publisher mistake or a stale clock. The interface words these
	/// differently from an ordinary network problem.
	pub const fn is_security_failure(self) -> bool {
		matches!(
			self,
			Self::NoTrustedSignature
				| Self::SignatureInvalid
				| Self::HashMismatch
				| Self::Rollback
				| Self::RevisionConflict
				| Self::WrongFeed
				| Self::DuplicateKeyId
		)
	}
}

/// A verification failure. `detail` is for diagnostics only: it can quote
/// attacker-controlled text and must not be shown as rich content.
#[derive(Debug, Error)]
#[error("{}: {detail}", kind.as_str())]
pub struct CatalogError {
	kind: ErrorKind,
	detail: String,
}

impl CatalogError {
	pub(crate) fn new(kind: ErrorKind, detail: impl Into<String>) -> Self {
		Self {
			kind,
			detail: detail.into(),
		}
	}

	pub fn kind(&self) -> ErrorKind {
		self.kind
	}

	pub fn detail(&self) -> &str {
		&self.detail
	}
}
