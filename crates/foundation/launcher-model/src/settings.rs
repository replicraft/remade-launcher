//! User preferences.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

/// Which color scheme the interface uses.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ThemePreference {
	/// Follow the operating system.
	#[default]
	System,
	Light,
	Dark,
}

impl ThemePreference {
	pub const ALL: [Self; 3] = [Self::System, Self::Light, Self::Dark];

	/// The stable string stored in the settings table.
	pub const fn as_str(self) -> &'static str {
		match self {
			Self::System => "system",
			Self::Light => "light",
			Self::Dark => "dark",
		}
	}

	/// The label shown in the interface.
	pub const fn label(self) -> &'static str {
		match self {
			Self::System => "System",
			Self::Light => "Light",
			Self::Dark => "Dark",
		}
	}
}

impl fmt::Display for ThemePreference {
	fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
		formatter.write_str(self.as_str())
	}
}

/// Returned when a stored value is not a known theme.
#[derive(Debug, PartialEq, Eq)]
pub struct UnknownTheme;

impl FromStr for ThemePreference {
	type Err = UnknownTheme;

	fn from_str(text: &str) -> Result<Self, Self::Err> {
		match text {
			"system" => Ok(Self::System),
			"light" => Ok(Self::Light),
			"dark" => Ok(Self::Dark),
			_ => Err(UnknownTheme),
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn every_theme_round_trips() {
		for theme in ThemePreference::ALL {
			assert_eq!(theme.as_str().parse(), Ok(theme));
		}
	}

	#[test]
	fn unknown_text_is_rejected() {
		assert_eq!("sepia".parse::<ThemePreference>(), Err(UnknownTheme));
	}
}
