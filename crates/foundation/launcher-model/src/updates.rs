//! Automatic-update policy values.
//!
//! These are plain preferences. None of them is a trust root: a user-writable
//! setting can delay or pause an update, never authorize one that
//! verification would refuse.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

/// Bump when the wording a person consented to changes, so old consent
/// records are recognizably for older text.
pub const CONSENT_POLICY_VERSION: u32 = 1;

/// The ways application updates can happen, from none to the most automatic.
/// The same chain is chosen for all apps in Settings and again for any one
/// app. Manual is the default.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum UpdateMode {
	/// Held at the version it has. Nothing updates it, not even on request,
	/// until the level is changed.
	Pinned,
	/// Check when the launcher opens and offer an Update button. Nothing is
	/// installed without an explicit action.
	#[default]
	Manual,
	/// Install eligible updates once per real launcher start.
	OnLaunch,
	/// A per-user background updater checks at login and every 24 hours.
	Daemon,
}

impl UpdateMode {
	/// Least to most automatic, which is the order they are offered in.
	pub const ALL: [Self; 4] = [Self::Pinned, Self::Manual, Self::OnLaunch, Self::Daemon];

	pub const fn as_str(self) -> &'static str {
		match self {
			Self::Pinned => "pinned",
			Self::Manual => "manual",
			Self::OnLaunch => "on-launch",
			Self::Daemon => "daemon",
		}
	}

	pub const fn label(self) -> &'static str {
		match self {
			Self::Pinned => "Pinned",
			Self::Manual => "Manual-update (Default)",
			Self::OnLaunch => "Auto-update on Launcher start",
			Self::Daemon => "Auto-update w/ always active daemon (Dangerous)",
		}
	}

	/// Whether this mode installs without a per-update confirmation.
	pub const fn is_automatic(self) -> bool {
		matches!(self, Self::OnLaunch | Self::Daemon)
	}

	/// The sentence shown under the option in Settings.
	pub const fn description(self) -> &'static str {
		match self {
			Self::Pinned => {
				"Stays at the version it has. Nothing updates it, and no Update button is offered, until you pick another level."
			}
			Self::Manual => {
				"Checks for updates when you open the Launcher. Installed applications show an Update Available button so you decide when to install."
			}
			Self::OnLaunch => {
				"Each time you start the Launcher, automatically installs eligible application updates. It does not keep running or update apps in the background after you close the Launcher."
			}
			Self::Daemon => {
				"Starts a lightweight background updater when you sign in. Checks on startup and every 24 hours while active, and also when you open the Launcher. Updates are installed without individual confirmation. Malicious, compromised, or broken upstream releases could be installed automatically and may damage software or data."
			}
		}
	}
}

impl fmt::Display for UpdateMode {
	fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
		formatter.write_str(self.as_str())
	}
}

#[derive(Debug, PartialEq, Eq)]
pub struct UnknownUpdateMode;

impl FromStr for UpdateMode {
	type Err = UnknownUpdateMode;

	fn from_str(text: &str) -> Result<Self, Self::Err> {
		match text {
			"pinned" => Ok(Self::Pinned),
			"manual" => Ok(Self::Manual),
			"on-launch" => Ok(Self::OnLaunch),
			"daemon" => Ok(Self::Daemon),
			_ => Err(UnknownUpdateMode),
		}
	}
}

/// How long a release must have existed before an automatic update may
/// install it. Stored as whole seconds.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct MinimumAge(u64);

impl MinimumAge {
	pub const NONE: Self = Self(0);
	/// The default: one hour.
	pub const DEFAULT: Self = Self(60 * 60);
	/// The longest allowed delay: thirty days.
	pub const MAX_SECONDS: u64 = 30 * 24 * 60 * 60;

	/// The presets offered in Settings, in order.
	pub const PRESETS: [(&'static str, Self); 7] = [
		("No delay", Self(0)),
		("1 hour", Self(60 * 60)),
		("6 hours", Self(6 * 60 * 60)),
		("12 hours", Self(12 * 60 * 60)),
		("24 hours", Self(24 * 60 * 60)),
		("3 days", Self(3 * 24 * 60 * 60)),
		("7 days", Self(7 * 24 * 60 * 60)),
	];

	/// `None` when `seconds` is past the maximum.
	pub const fn from_seconds(seconds: u64) -> Option<Self> {
		if seconds <= Self::MAX_SECONDS {
			Some(Self(seconds))
		} else {
			None
		}
	}

	pub const fn seconds(self) -> u64 {
		self.0
	}

	/// A short human form: `0`, `90s`, `45m`, `1h`, `36h`, `3d`.
	pub fn short(self) -> String {
		let seconds = self.0;

		match seconds {
			0 => "0".to_owned(),
			s if s % 86_400 == 0 => format!("{}d", s / 86_400),
			s if s % 3_600 == 0 => format!("{}h", s / 3_600),
			s if s % 60 == 0 => format!("{}m", s / 60),
			s => format!("{s}s"),
		}
	}
}

impl Default for MinimumAge {
	fn default() -> Self {
		Self::DEFAULT
	}
}

impl fmt::Display for MinimumAge {
	fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
		formatter.write_str(&self.short())
	}
}

#[derive(Debug, PartialEq, Eq)]
pub enum AgeParseError {
	Empty,
	NotANumber,
	UnknownUnit,
	TooLong,
}

impl fmt::Display for AgeParseError {
	fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
		formatter.write_str(match self {
			Self::Empty => "no duration was given",
			Self::NotANumber => "the duration must start with a whole number, such as 1h",
			Self::UnknownUnit => "use s, m, h, or d, such as 30m, 1h, or 3d",
			Self::TooLong => "the longest delay is 30 days",
		})
	}
}

impl FromStr for MinimumAge {
	type Err = AgeParseError;

	/// Accepts `0`, or a whole number followed by `s`, `m`, `h`, or `d`.
	fn from_str(text: &str) -> Result<Self, Self::Err> {
		let text = text.trim();

		if text.is_empty() {
			return Err(AgeParseError::Empty);
		}

		if text == "0" {
			return Ok(Self::NONE);
		}

		let split = text
			.find(|c: char| !c.is_ascii_digit())
			.unwrap_or(text.len());
		let (digits, unit) = text.split_at(split);

		let amount: u64 = digits.parse().map_err(|_| AgeParseError::NotANumber)?;
		let multiplier = match unit {
			"s" => 1,
			"m" => 60,
			"h" => 3_600,
			"d" => 86_400,
			_ => return Err(AgeParseError::UnknownUnit),
		};

		amount
			.checked_mul(multiplier)
			.and_then(Self::from_seconds)
			.ok_or(AgeParseError::TooLong)
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn modes_round_trip_and_manual_is_the_default() {
		assert_eq!(UpdateMode::default(), UpdateMode::Manual);

		for mode in UpdateMode::ALL {
			assert_eq!(mode.as_str().parse(), Ok(mode));
		}

		assert!("always".parse::<UpdateMode>().is_err());
		assert!(!UpdateMode::Pinned.is_automatic());
		assert!(!UpdateMode::Manual.is_automatic());
		assert!(UpdateMode::OnLaunch.is_automatic() && UpdateMode::Daemon.is_automatic());
	}

	#[test]
	fn the_default_age_is_one_hour() {
		assert_eq!(MinimumAge::default().seconds(), 3_600);
		assert_eq!(MinimumAge::PRESETS[1].1, MinimumAge::DEFAULT);
	}

	#[test]
	fn durations_parse_and_print() {
		for (text, seconds) in [
			("0", 0),
			("90s", 90),
			("45m", 2_700),
			("1h", 3_600),
			("36h", 129_600),
			("3d", 259_200),
			(" 7d ", 604_800),
		] {
			assert_eq!(
				text.parse::<MinimumAge>().unwrap().seconds(),
				seconds,
				"{text}"
			);
		}

		assert_eq!(MinimumAge::from_seconds(3_600).unwrap().short(), "1h");
		assert_eq!(MinimumAge::from_seconds(90).unwrap().short(), "90s");
		assert_eq!(MinimumAge::from_seconds(259_200).unwrap().short(), "3d");
		assert_eq!(MinimumAge::NONE.short(), "0");
	}

	#[test]
	fn bad_durations_are_refused_with_a_reason() {
		assert_eq!("".parse::<MinimumAge>(), Err(AgeParseError::Empty));
		assert_eq!("h".parse::<MinimumAge>(), Err(AgeParseError::NotANumber));
		assert_eq!("1w".parse::<MinimumAge>(), Err(AgeParseError::UnknownUnit));
		assert_eq!("5".parse::<MinimumAge>(), Err(AgeParseError::UnknownUnit));
		assert_eq!("31d".parse::<MinimumAge>(), Err(AgeParseError::TooLong));
		assert_eq!(
			"99999999999999999999h".parse::<MinimumAge>(),
			Err(AgeParseError::NotANumber)
		);
		assert_eq!("-1h".parse::<MinimumAge>(), Err(AgeParseError::NotANumber));
	}

	#[test]
	fn a_zero_delay_is_a_real_choice() {
		assert_eq!(MinimumAge::PRESETS[0].1, MinimumAge::NONE);
	}
}
