//! Everything a person can set in Settings, other than the theme and the
//! update mode, which have their own stores.
//!
//! The whole set is kept as one JSON document. Every field has a default and
//! unknown or missing fields are tolerated, so adding a setting later never
//! breaks an older saved copy.

use serde::{Deserialize, Serialize};

pub use crate::local_time::{DateFormat, TimeFormat};

/// The interface language. Only English has strings today; the setting is
/// here so the choice survives once there are more.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Language {
	/// Pick from the system's language, falling back to English.
	#[default]
	Auto,
	English,
}

impl Language {
	pub const ALL: [Self; 2] = [Self::Auto, Self::English];

	pub const fn label(self) -> &'static str {
		match self {
			Self::Auto => "Automatic",
			Self::English => "English",
		}
	}

	/// The language the interface will actually use for `system_tag`.
	pub fn resolve(self, _system_tag: &str) -> Self {
		// Every system language falls back to English until there are
		// translations to choose from.
		Self::English
	}
}

/// Which page the launcher opens on.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StartPage {
	/// The Library if anything is installed, otherwise Explore.
	#[default]
	Auto,
	Library,
	Explore,
	/// Wherever the person was last.
	LastUsed,
}

impl StartPage {
	pub const ALL: [Self; 4] = [Self::Auto, Self::Library, Self::Explore, Self::LastUsed];

	pub const fn label(self) -> &'static str {
		match self {
			Self::Auto => "Library if I have apps, otherwise Explore",
			Self::Library => "Library",
			Self::Explore => "Explore",
			Self::LastUsed => "Where I left off",
		}
	}
}

/// What the launcher remembers about how it was laid out.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Layout {
	pub sidebar_collapsed: bool,
	/// Cards rather than rows.
	pub grid_view: bool,
	pub explore_sort: String,
	pub library_sort: String,
	/// The page the launcher was last on, for [`StartPage::LastUsed`].
	pub last_page: String,
	/// The window's inner size in points, when it was last closed.
	pub window: Option<(f32, f32)>,
}

impl Default for Layout {
	fn default() -> Self {
		Self {
			sidebar_collapsed: false,
			grid_view: true,
			explore_sort: String::new(),
			library_sort: String::new(),
			last_page: String::new(),
			window: None,
		}
	}
}

/// Which desktop notifications to send. Both are off until asked for.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Notifications {
	/// A check found updates.
	pub updates_available: bool,
	/// An install or update finished or failed while the window is in the
	/// background.
	pub install_finished: bool,
}

/// How the launcher reaches the internet.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "mode", content = "url")]
pub enum Proxy {
	/// Whatever the system or environment says.
	#[default]
	System,
	/// Connect directly, ignoring any proxy.
	Direct,
	/// An `http://`, `https://`, or `socks5://` address, without credentials.
	Custom(String),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Network {
	pub proxy: Proxy,
	/// How long a request may take, in seconds.
	pub timeout_seconds: u32,
	/// A GitHub token is saved in the system's credential store. The store
	/// is not touched at all unless this is set, so a launcher that never
	/// had a token never asks the system for one.
	pub token_saved: bool,
}

impl Default for Network {
	fn default() -> Self {
		Self {
			proxy: Proxy::System,
			timeout_seconds: 30,
			token_saved: false,
		}
	}
}

impl Network {
	pub const MIN_TIMEOUT: u32 = 5;
	pub const MAX_TIMEOUT: u32 = 300;
}

/// Why a proxy address was not accepted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProxyProblem {
	Empty,
	NotAnAddress,
	UnsupportedScheme,
	/// A user name or password in the address would be stored in plain text.
	HasCredentials,
}

impl ProxyProblem {
	pub const fn message(self) -> &'static str {
		match self {
			Self::Empty => "Enter an address, such as http://proxy.example:8080.",
			Self::NotAnAddress => "That is not a valid address.",
			Self::UnsupportedScheme => "Use an http://, https://, or socks5:// address.",
			Self::HasCredentials => {
				"Leave the user name and password out. They would be saved unprotected."
			}
		}
	}
}

/// Checks a proxy address without contacting it.
pub fn check_proxy(text: &str) -> Result<(), ProxyProblem> {
	let text = text.trim();

	if text.is_empty() {
		return Err(ProxyProblem::Empty);
	}

	let (scheme, rest) = text.split_once("://").ok_or(ProxyProblem::NotAnAddress)?;

	if !matches!(
		scheme.to_ascii_lowercase().as_str(),
		"http" | "https" | "socks5" | "socks5h"
	) {
		return Err(ProxyProblem::UnsupportedScheme);
	}

	let authority = rest.split(['/', '?', '#']).next().unwrap_or("");

	if authority.contains('@') {
		return Err(ProxyProblem::HasCredentials);
	}

	let (host, port) = match authority.rsplit_once(':') {
		Some((host, port)) => (host, Some(port)),
		None => (authority, None),
	};

	let host_ok = !host.is_empty()
		&& host
			.chars()
			.all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '[' | ']' | ':'));
	let port_ok = port.is_none_or(|port| port.parse::<u16>().is_ok());

	if host_ok && port_ok {
		Ok(())
	} else {
		Err(ProxyProblem::NotAnAddress)
	}
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Preferences {
	pub language: Language,
	pub time_format: TimeFormat,
	pub date_format: DateFormat,
	/// Interface size as a percentage.
	pub ui_scale: u16,
	pub start_page: StartPage,
	pub layout: Layout,
	pub notifications: Notifications,
	pub network: Network,
}

impl Default for Preferences {
	fn default() -> Self {
		Self {
			language: Language::Auto,
			time_format: TimeFormat::System,
			date_format: DateFormat::System,
			ui_scale: Self::DEFAULT_SCALE,
			start_page: StartPage::Auto,
			layout: Layout::default(),
			notifications: Notifications::default(),
			network: Network::default(),
		}
	}
}

impl Preferences {
	pub const DEFAULT_SCALE: u16 = 100;
	/// The sizes offered in Settings.
	pub const SCALES: [u16; 5] = [90, 100, 110, 125, 150];

	/// A copy that is safe to use: out-of-range numbers are brought into
	/// range and a proxy address that would not be accepted is dropped back
	/// to the system setting.
	pub fn sanitized(mut self) -> Self {
		self.ui_scale = self.ui_scale.clamp(75, 200);
		self.network.timeout_seconds = self
			.network
			.timeout_seconds
			.clamp(Network::MIN_TIMEOUT, Network::MAX_TIMEOUT);

		if let Proxy::Custom(address) = &self.network.proxy
			&& check_proxy(address).is_err()
		{
			self.network.proxy = Proxy::System;
		}

		if let Some((width, height)) = self.layout.window
			&& !(width.is_finite()
				&& height.is_finite()
				&& (320.0..=10_000.0).contains(&width)
				&& (240.0..=10_000.0).contains(&height))
		{
			self.layout.window = None;
		}

		self
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn the_defaults_are_sensible() {
		let defaults = Preferences::default();

		assert_eq!(defaults.ui_scale, 100);
		assert!(defaults.layout.grid_view);
		assert!(
			!defaults.notifications.updates_available && !defaults.notifications.install_finished
		);
		assert_eq!(defaults.network.proxy, Proxy::System);
		assert_eq!(defaults.network.timeout_seconds, 30);
	}

	#[test]
	fn an_empty_or_partial_document_fills_in_the_rest() {
		let empty: Preferences = serde_json::from_str("{}").unwrap();
		let partial: Preferences =
			serde_json::from_str(r#"{"ui_scale":125,"layout":{"sidebar_collapsed":true}}"#)
				.unwrap();

		assert_eq!(empty, Preferences::default());
		assert_eq!(partial.ui_scale, 125);
		assert!(partial.layout.sidebar_collapsed);
		assert!(
			partial.layout.grid_view,
			"an unmentioned field keeps its default"
		);
	}

	#[test]
	fn a_round_trip_keeps_everything() {
		let mut prefs = Preferences {
			time_format: TimeFormat::Hour24,
			date_format: DateFormat::Iso,
			..Preferences::default()
		};

		prefs.network.proxy = Proxy::Custom("socks5://127.0.0.1:1080".into());
		prefs.layout.window = Some((1300.0, 840.0));

		let json = serde_json::to_string(&prefs).unwrap();

		assert_eq!(serde_json::from_str::<Preferences>(&json).unwrap(), prefs);
	}

	#[test]
	fn proxy_addresses_are_checked_without_connecting() {
		for good in [
			"http://proxy.example:8080",
			"https://proxy.example",
			"socks5://127.0.0.1:1080",
			"http://[::1]:3128",
		] {
			assert_eq!(check_proxy(good), Ok(()), "{good}");
		}

		assert_eq!(check_proxy(""), Err(ProxyProblem::Empty));
		assert_eq!(
			check_proxy("proxy.example:8080"),
			Err(ProxyProblem::NotAnAddress)
		);
		assert_eq!(
			check_proxy("ftp://proxy.example"),
			Err(ProxyProblem::UnsupportedScheme)
		);
		assert_eq!(
			check_proxy("http://user:secret@proxy.example"),
			Err(ProxyProblem::HasCredentials)
		);
		assert_eq!(
			check_proxy("http://proxy.example:99999"),
			Err(ProxyProblem::NotAnAddress)
		);
		assert_eq!(check_proxy("http://"), Err(ProxyProblem::NotAnAddress));
	}

	#[test]
	fn sanitizing_repairs_values_that_could_cause_trouble() {
		let mut prefs = Preferences {
			ui_scale: 5_000,
			..Preferences::default()
		};

		prefs.network.timeout_seconds = 0;
		prefs.network.proxy = Proxy::Custom("http://user:pw@host".into());
		prefs.layout.window = Some((f32::NAN, 100.0));

		let clean = prefs.sanitized();

		assert_eq!(clean.ui_scale, 200);
		assert_eq!(clean.network.timeout_seconds, Network::MIN_TIMEOUT);
		assert_eq!(clean.network.proxy, Proxy::System);
		assert_eq!(clean.layout.window, None);
	}
}
