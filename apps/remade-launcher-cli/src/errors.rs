//! The typed error taxonomy scripts rely on.
//!
//! Every failure has a stable upper-case code and an exit status. Messages
//! are for people and may change; codes may not.

use launcher_model::{ErrorCode, LauncherError, Retry};

use crate::exit::ExitStatus;

#[derive(Debug)]
pub(crate) struct CliError {
	pub(crate) code: &'static str,
	pub(crate) status: ExitStatus,
	pub(crate) message: String,
	pub(crate) retryable: bool,
}

impl CliError {
	pub(crate) fn new(code: &'static str, status: ExitStatus, message: impl Into<String>) -> Self {
		Self {
			code,
			status,
			message: message.into(),
			retryable: false,
		}
	}

	pub(crate) fn usage(message: impl Into<String>) -> Self {
		Self::new("USAGE", ExitStatus::Usage, message)
	}

	pub(crate) fn approval(message: impl Into<String>) -> Self {
		Self::new("APPROVAL_REQUIRED", ExitStatus::ApprovalRequired, message)
	}
}

impl From<LauncherError> for CliError {
	fn from(error: LauncherError) -> Self {
		let (code, status) = classify(error.code());

		Self {
			code,
			status,
			message: error.user_message().to_owned(),
			retryable: error.retry() == Retry::Later,
		}
	}
}

/// The code and exit status for an engine error.
pub(crate) fn classify(code: ErrorCode) -> (&'static str, ExitStatus) {
	use ExitStatus::{ApprovalRequired, Conflict, Internal, Network, Trust, Unsupported, Usage};

	match code {
		ErrorCode::NetworkRequestFailed
		| ErrorCode::NetworkResponseTooLarge
		| ErrorCode::ProviderDenied
		| ErrorCode::ProviderNotFound
		| ErrorCode::ProviderResponseMalformed => ("RELEASE_LOOKUP_FAILED", Network),
		ErrorCode::ProviderRateLimited => ("RATE_LIMITED", Network),
		ErrorCode::SourceIdentityChanged => ("SOURCE_IDENTITY_CHANGED", Trust),
		ErrorCode::InvalidSourceUrl => ("INVALID_SOURCE_URL", Usage),
		ErrorCode::ArtifactDigestMismatch => ("ARTIFACT_DIGEST_MISMATCH", Trust),
		ErrorCode::PackageUnsafe => ("PACKAGE_UNSAFE", Trust),
		ErrorCode::PackageUnsupported => ("PLATFORM_INCOMPATIBLE", Unsupported),
		ErrorCode::InstallationNotFound => ("INSTALLATION_NOT_FOUND", Usage),
		ErrorCode::InstallationChanged => ("INSTALLATION_OWNERSHIP_CONFLICT", Conflict),
		ErrorCode::LaunchFailed => ("LAUNCH_FAILED", Unsupported),
		ErrorCode::OperationInProgress => ("OPERATION_IN_PROGRESS", Conflict),
		ErrorCode::ConfirmationRequired => ("APPROVAL_REQUIRED", ApprovalRequired),
		ErrorCode::ConsentRequired => ("CONSENT_REQUIRED", ApprovalRequired),
		ErrorCode::SourceApprovalRequired => {
			("UNVERIFIED_SOURCE_APPROVAL_REQUIRED", ApprovalRequired)
		}
		ErrorCode::PlanExpired => ("PLAN_EXPIRED", ApprovalRequired),
		ErrorCode::PlanChanged => ("PLAN_CHANGED", Conflict),
		ErrorCode::AppPinned => ("APP_PINNED", Conflict),
		ErrorCode::StorageNewerThanApp => ("UNSUPPORTED_SCHEMA", Unsupported),
		ErrorCode::StorageUnavailable | ErrorCode::StorageMigrationFailed => {
			("DATA_INTEGRITY_FAILURE", Internal)
		}
		ErrorCode::InternalInvariant => ("INTERNAL", Internal),
	}
}

/// The status a catalog problem maps to.
pub(crate) fn catalog_status(code: &str) -> ExitStatus {
	match code {
		"RELEASE_LOOKUP_FAILED" => ExitStatus::Network,
		"DATA_INTEGRITY_FAILURE" => ExitStatus::Internal,
		"UNSUPPORTED_SCHEMA" => ExitStatus::Unsupported,
		_ => ExitStatus::Trust,
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn every_engine_error_has_a_code_and_a_status() {
		// Exhaustive on purpose: adding an ErrorCode must force a decision here.
		let all = [
			ErrorCode::NetworkRequestFailed,
			ErrorCode::NetworkResponseTooLarge,
			ErrorCode::ProviderDenied,
			ErrorCode::ProviderRateLimited,
			ErrorCode::ProviderNotFound,
			ErrorCode::ProviderResponseMalformed,
			ErrorCode::SourceIdentityChanged,
			ErrorCode::InvalidSourceUrl,
			ErrorCode::ArtifactDigestMismatch,
			ErrorCode::PackageUnsafe,
			ErrorCode::PackageUnsupported,
			ErrorCode::InstallationNotFound,
			ErrorCode::InstallationChanged,
			ErrorCode::LaunchFailed,
			ErrorCode::OperationInProgress,
			ErrorCode::ConfirmationRequired,
			ErrorCode::ConsentRequired,
			ErrorCode::SourceApprovalRequired,
			ErrorCode::PlanExpired,
			ErrorCode::PlanChanged,
			ErrorCode::AppPinned,
			ErrorCode::StorageUnavailable,
			ErrorCode::StorageMigrationFailed,
			ErrorCode::StorageNewerThanApp,
			ErrorCode::InternalInvariant,
		];

		for code in all {
			let (name, status) = classify(code);

			assert!(
				!name.is_empty() && name.chars().all(|c| c.is_ascii_uppercase() || c == '_'),
				"{code:?}: {name}"
			);
			assert_ne!(status, ExitStatus::Success);
		}
	}

	#[test]
	fn security_failures_exit_as_trust_failures() {
		assert_eq!(
			classify(ErrorCode::ArtifactDigestMismatch).1,
			ExitStatus::Trust
		);
		assert_eq!(
			classify(ErrorCode::SourceIdentityChanged).1,
			ExitStatus::Trust
		);
		assert_eq!(
			classify(ErrorCode::SourceApprovalRequired).1,
			ExitStatus::ApprovalRequired
		);
		assert_eq!(
			classify(ErrorCode::OperationInProgress).1,
			ExitStatus::Conflict
		);
		assert_eq!(classify(ErrorCode::ProviderRateLimited).0, "RATE_LIMITED");
	}

	#[test]
	fn catalog_codes_map_to_statuses() {
		assert_eq!(
			catalog_status("CATALOG_SIGNATURE_INVALID"),
			ExitStatus::Trust
		);
		assert_eq!(catalog_status("CATALOG_EXPIRED"), ExitStatus::Trust);
		assert_eq!(catalog_status("RELEASE_LOOKUP_FAILED"), ExitStatus::Network);
	}
}
