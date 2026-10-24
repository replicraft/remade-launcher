//! Exit statuses.
//!
//! Scripts branch on these, so they are named constants with tests rather
//! than numbers scattered through the code.

use std::process::ExitCode;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum ExitStatus {
	Success = 0,
	/// Bad arguments or input.
	Usage = 2,
	/// The user declined, or an approval is missing.
	ApprovalRequired = 3,
	/// No compatible package, or an unsupported platform or feature.
	Unsupported = 4,
	/// The network, the provider, or a rate limit.
	Network = 5,
	/// Something failed a trust or security check.
	Trust = 6,
	/// A conflict, or another operation is running.
	Conflict = 7,
	/// Finished only in part.
	Incomplete = 8,
	/// A previous operation needs recovery first.
	RecoveryRequired = 9,
	/// A bug or an unexpected failure.
	Internal = 10,
}

impl ExitStatus {
	pub(crate) fn code(self) -> u8 {
		self as u8
	}

	/// The worse of two statuses, for a batch where some parts failed.
	pub(crate) fn worst(self, other: Self) -> Self {
		self.max(other)
	}
}

impl From<ExitStatus> for ExitCode {
	fn from(status: ExitStatus) -> Self {
		ExitCode::from(status.code())
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn the_numbers_are_the_documented_contract() {
		let documented = [
			(ExitStatus::Success, 0),
			(ExitStatus::Usage, 2),
			(ExitStatus::ApprovalRequired, 3),
			(ExitStatus::Unsupported, 4),
			(ExitStatus::Network, 5),
			(ExitStatus::Trust, 6),
			(ExitStatus::Conflict, 7),
			(ExitStatus::Incomplete, 8),
			(ExitStatus::RecoveryRequired, 9),
			(ExitStatus::Internal, 10),
		];

		for (status, number) in documented {
			assert_eq!(status.code(), number, "{status:?}");
		}
	}

	#[test]
	fn a_partly_failed_batch_never_reports_success() {
		assert_eq!(
			ExitStatus::Success.worst(ExitStatus::Network),
			ExitStatus::Network
		);
		assert_eq!(
			ExitStatus::Trust.worst(ExitStatus::Network),
			ExitStatus::Trust
		);
		assert_eq!(
			ExitStatus::Success.worst(ExitStatus::Success),
			ExitStatus::Success
		);
	}
}
