//! The error taxonomy.
//!
//! Every error carries a stable machine-readable code, a category, a retry
//! hint, and a message that is safe to show to a user. Raw causes stay in the
//! source chain for diagnostics and must be redacted before they are shown or
//! exported.

use std::error::Error as StdError;

use serde::{Deserialize, Serialize};

pub type Result<T, E = LauncherError> = std::result::Result<T, E>;

/// Broad families of failure, each with its own recovery story.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ErrorCategory {
	Network,
	Trust,
	Compatibility,
	Ambiguity,
	LocalOwnership,
	Resource,
	Privilege,
	NativeInstaller,
	Recovery,
	Storage,
	Internal,
}

/// Whether trying again can help.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Retry {
	/// Retrying will not change the outcome.
	Never,
	/// Retry after a delay, for example a timeout or a rate limit.
	Later,
	/// Retry once the user has changed something, such as freeing disk space.
	AfterUserAction,
}

/// Stable identifiers for specific failures. Localized text is keyed off
/// these, so existing variants must never be renamed or reused.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ErrorCode {
	NetworkRequestFailed,
	NetworkResponseTooLarge,
	ProviderDenied,
	ProviderRateLimited,
	ProviderNotFound,
	ProviderResponseMalformed,
	SourceIdentityChanged,
	InvalidSourceUrl,
	ArtifactDigestMismatch,
	PackageUnsafe,
	PackageUnsupported,
	InstallationNotFound,
	InstallationChanged,
	LaunchFailed,
	OperationInProgress,
	Cancelled,
	AdoptionRefused,
	/// The launcher cannot replace itself here, for example because a package
	/// manager owns it or this build has no release key.
	SelfUpdateUnavailable,
	/// A launcher release failed a signature or rollback check.
	ReleaseRejected,
	ConfirmationRequired,
	ConsentRequired,
	SourceApprovalRequired,
	PlanExpired,
	PlanChanged,
	AppPinned,
	StorageUnavailable,
	StorageMigrationFailed,
	StorageNewerThanApp,
	InternalInvariant,
}

impl ErrorCode {
	/// The stable string form used in logs and diagnostics.
	pub const fn as_str(self) -> &'static str {
		match self {
			Self::NetworkRequestFailed => "network.request_failed",
			Self::NetworkResponseTooLarge => "network.response_too_large",
			Self::ProviderDenied => "provider.denied",
			Self::ProviderRateLimited => "provider.rate_limited",
			Self::ProviderNotFound => "provider.not_found",
			Self::ProviderResponseMalformed => "provider.response_malformed",
			Self::SourceIdentityChanged => "provider.source_identity_changed",
			Self::InvalidSourceUrl => "source.invalid_url",
			Self::ArtifactDigestMismatch => "artifact.digest_mismatch",
			Self::PackageUnsafe => "package.unsafe",
			Self::PackageUnsupported => "package.unsupported",
			Self::InstallationNotFound => "installation.not_found",
			Self::InstallationChanged => "installation.changed",
			Self::LaunchFailed => "installation.launch_failed",
			Self::OperationInProgress => "operation.in_progress",
			Self::Cancelled => "operation.cancelled",
			Self::AdoptionRefused => "installation.adoption_refused",
			Self::SelfUpdateUnavailable => "selfupdate.unavailable",
			Self::ReleaseRejected => "selfupdate.release_rejected",
			Self::ConfirmationRequired => "cli.confirmation_required",
			Self::ConsentRequired => "policy.consent_required",
			Self::SourceApprovalRequired => "policy.source_approval_required",
			Self::PlanExpired => "plan.expired",
			Self::PlanChanged => "plan.changed",
			Self::AppPinned => "installation.pinned",
			Self::StorageUnavailable => "storage.unavailable",
			Self::StorageMigrationFailed => "storage.migration_failed",
			Self::StorageNewerThanApp => "storage.newer_than_app",
			Self::InternalInvariant => "internal.invariant",
		}
	}

	pub const fn category(self) -> ErrorCategory {
		match self {
			Self::NetworkRequestFailed
			| Self::NetworkResponseTooLarge
			| Self::ProviderDenied
			| Self::ProviderRateLimited
			| Self::ProviderNotFound
			| Self::ProviderResponseMalformed => ErrorCategory::Network,
			Self::SourceIdentityChanged | Self::ArtifactDigestMismatch | Self::ReleaseRejected => {
				ErrorCategory::Trust
			}
			Self::SelfUpdateUnavailable => ErrorCategory::Compatibility,
			Self::PackageUnsafe | Self::PackageUnsupported => ErrorCategory::Compatibility,
			Self::InstallationNotFound
			| Self::InstallationChanged
			| Self::LaunchFailed
			| Self::OperationInProgress
			| Self::Cancelled
			| Self::AdoptionRefused => ErrorCategory::LocalOwnership,
			Self::InvalidSourceUrl
			| Self::ConfirmationRequired
			| Self::ConsentRequired
			| Self::SourceApprovalRequired => ErrorCategory::Ambiguity,
			Self::PlanExpired | Self::PlanChanged => ErrorCategory::Trust,
			Self::AppPinned => ErrorCategory::LocalOwnership,
			Self::StorageUnavailable | Self::StorageMigrationFailed | Self::StorageNewerThanApp => {
				ErrorCategory::Storage
			}
			Self::InternalInvariant => ErrorCategory::Internal,
		}
	}

	pub const fn retry(self) -> Retry {
		match self {
			Self::NetworkRequestFailed | Self::ProviderRateLimited => Retry::Later,
			Self::NetworkResponseTooLarge
			| Self::StorageMigrationFailed
			| Self::ProviderDenied
			| Self::ProviderNotFound
			| Self::ProviderResponseMalformed
			| Self::SourceIdentityChanged
			| Self::ArtifactDigestMismatch
			| Self::ReleaseRejected
			| Self::PackageUnsafe
			| Self::PackageUnsupported
			| Self::SelfUpdateUnavailable
			| Self::InstallationNotFound => Retry::Never,
			Self::InstallationChanged | Self::LaunchFailed => Retry::AfterUserAction,
			Self::OperationInProgress => Retry::Later,
			Self::Cancelled | Self::AdoptionRefused => Retry::AfterUserAction,
			Self::InvalidSourceUrl
			| Self::ConfirmationRequired
			| Self::ConsentRequired
			| Self::SourceApprovalRequired => Retry::AfterUserAction,
			Self::PlanExpired | Self::PlanChanged => Retry::AfterUserAction,
			Self::AppPinned => Retry::AfterUserAction,
			Self::StorageUnavailable | Self::StorageNewerThanApp => Retry::AfterUserAction,
			Self::InternalInvariant => Retry::Never,
		}
	}
}

/// An error with a stable code and a user-safe message.
#[derive(Debug, thiserror::Error)]
#[error("{code}: {message}", code = .code.as_str())]
pub struct LauncherError {
	code: ErrorCode,
	message: String,
	#[source]
	source: Option<Box<dyn StdError + Send + Sync>>,
}

impl LauncherError {
	/// Creates an error. `message` must be safe to show to a user: no paths,
	/// tokens, or raw upstream text.
	pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
		Self {
			code,
			message: message.into(),
			source: None,
		}
	}

	/// Attaches the underlying cause for diagnostics.
	pub fn with_source(mut self, source: impl StdError + Send + Sync + 'static) -> Self {
		self.source = Some(Box::new(source));
		self
	}

	pub fn code(&self) -> ErrorCode {
		self.code
	}

	pub fn category(&self) -> ErrorCategory {
		self.code.category()
	}

	pub fn retry(&self) -> Retry {
		self.code.retry()
	}

	/// The message that is safe to show in the interface.
	pub fn user_message(&self) -> &str {
		&self.message
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn exposes_code_category_and_retry() {
		let error = LauncherError::new(
			ErrorCode::NetworkRequestFailed,
			"The server did not answer.",
		);

		assert_eq!(error.category(), ErrorCategory::Network);
		assert_eq!(error.retry(), Retry::Later);
		assert_eq!(
			error.to_string(),
			"network.request_failed: The server did not answer."
		);
	}

	#[test]
	fn keeps_the_cause_out_of_the_user_message() {
		let cause = std::io::Error::other("/Users/someone/secret/path");
		let error = LauncherError::new(
			ErrorCode::StorageUnavailable,
			"Storage could not be opened.",
		)
		.with_source(cause);

		assert!(!error.user_message().contains("secret"));
		assert!(std::error::Error::source(&error).is_some());
	}
}
