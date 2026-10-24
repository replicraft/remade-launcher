//! Everything a person can set in Settings, other than the theme and the
//! update mode, which have their own stores.
//!
//! The whole set is kept as one JSON document. Every field has a default and
//! unknown or missing fields are tolerated, so adding a setting later never
//! breaks an older saved copy.

use serde::{Deserialize, Serialize};

pub use crate::local_time::{DateFormat, TimeFormat};

/// The interface language.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Language {
	/// Pick from the system's language, falling back to English.
	#[default]
	Auto,
	English,
	Japanese,
	PortugueseBrazil,
	ChineseSimplified,
	German,
	Hindi,
}

impl Language {
	pub const ALL: [Self; 7] = [
		Self::Auto,
		Self::English,
		Self::Japanese,
		Self::PortugueseBrazil,
		Self::ChineseSimplified,
		Self::German,
		Self::Hindi,
	];

	/// The languages with translations, which `Auto` chooses among.
	pub const TRANSLATED: [Self; 6] = [
		Self::English,
		Self::Japanese,
		Self::PortugueseBrazil,
		Self::ChineseSimplified,
		Self::German,
		Self::Hindi,
	];

	/// What the language is called in itself, so a person can find theirs
	/// whatever the interface says now. `Auto` has no name of its own: the
	/// interface words it in the current language.
	pub const fn endonym(self) -> Option<&'static str> {
		match self {
			Self::Auto => None,
			Self::English => Some("English"),
			Self::Japanese => Some("日本語"),
			Self::PortugueseBrazil => Some("Português (Brasil)"),
			Self::ChineseSimplified => Some("简体中文"),
			Self::German => Some("Deutsch"),
			Self::Hindi => Some("हिन्दी"),
		}
	}

	pub const fn as_str(self) -> &'static str {
		match self {
			Self::Auto => "auto",
			Self::English => "english",
			Self::Japanese => "japanese",
			Self::PortugueseBrazil => "portuguese_brazil",
			Self::ChineseSimplified => "chinese_simplified",
			Self::German => "german",
			Self::Hindi => "hindi",
		}
	}

	/// The BCP 47 tag of a translated language. `Auto` has none.
	pub const fn tag(self) -> Option<&'static str> {
		match self {
			Self::Auto => None,
			Self::English => Some("en"),
			Self::Japanese => Some("ja"),
			Self::PortugueseBrazil => Some("pt-BR"),
			Self::ChineseSimplified => Some("zh-CN"),
			Self::German => Some("de"),
			Self::Hindi => Some("hi"),
		}
	}

	/// The language the interface will actually use. A fixed choice is kept;
	/// `Auto` follows `system_tag` (such as `de-AT` or `zh_Hans_CN`) and falls
	/// back to English for anything without a translation.
	pub fn resolve(self, system_tag: &str) -> Self {
		if self != Self::Auto {
			return self;
		}

		let primary = system_tag
			.split(['-', '_'])
			.next()
			.unwrap_or("")
			.to_ascii_lowercase();

		match primary.as_str() {
			"ja" => Self::Japanese,
			"pt" => Self::PortugueseBrazil,
			"zh" => Self::ChineseSimplified,
			"de" => Self::German,
			"hi" => Self::Hindi,
			_ => Self::English,
		}
	}
}

impl std::str::FromStr for Language {
	type Err = ();

	/// Accepts the stored name, or a language tag such as `pt-br` or `ja`.
	fn from_str(text: &str) -> Result<Self, ()> {
		let text = text.trim().to_ascii_lowercase().replace('_', "-");

		match text.as_str() {
			"auto" => Ok(Self::Auto),
			"english" | "en" | "en-us" | "en-gb" => Ok(Self::English),
			"japanese" | "ja" | "ja-jp" => Ok(Self::Japanese),
			"portuguese-brazil" | "portuguese" | "pt" | "pt-br" => Ok(Self::PortugueseBrazil),
			"chinese-simplified" | "chinese" | "zh" | "zh-cn" | "zh-hans" => {
				Ok(Self::ChineseSimplified)
			}
			"german" | "de" | "de-de" => Ok(Self::German),
			"hindi" | "hi" | "hi-in" => Ok(Self::Hindi),
			_ => Err(()),
		}
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

	/// A stable key, for looking up the translated label.
	pub const fn as_str(self) -> &'static str {
		match self {
			Self::Auto => "auto",
			Self::Library => "library",
			Self::Explore => "explore",
			Self::LastUsed => "last-used",
		}
	}

	pub const fn label(self) -> &'static str {
		match self {
			Self::Auto => "Library if I have apps, otherwise Explore",
			Self::Library => "Library",
			Self::Explore => "Explore",
			Self::LastUsed => "Where I left off",
		}
	}
}

/// How the launcher treats updates to itself.
///
/// This is its own choice, apart from how installed apps are updated. Turning
/// app updates to manual does not stop the launcher from keeping itself
/// current, and the other way round.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LauncherUpdates {
	/// Download and check a new launcher in the background, and switch to it
	/// the next time the launcher is closed.
	#[default]
	Automatic,
	/// Say that a new launcher is available. Nothing is downloaded.
	Notify,
	/// Never look for one.
	Off,
}

impl LauncherUpdates {
	pub const ALL: [Self; 3] = [Self::Automatic, Self::Notify, Self::Off];

	pub const fn label(self) -> &'static str {
		match self {
			Self::Automatic => "Automatic",
			Self::Notify => "Tell me",
			Self::Off => "Off",
		}
	}

	pub const fn as_str(self) -> &'static str {
		match self {
			Self::Automatic => "automatic",
			Self::Notify => "notify",
			Self::Off => "off",
		}
	}

	pub const fn description(self) -> &'static str {
		match self {
			Self::Automatic => {
				"Download a new launcher in the background and switch to it when you close this one."
			}
			Self::Notify => "Say when a new launcher is available, and leave the rest to you.",
			Self::Off => "Never look for a new launcher.",
		}
	}

	/// Whether the launcher may look for a new release at all.
	pub const fn checks(self) -> bool {
		!matches!(self, Self::Off)
	}

	/// Whether it may download one without being asked.
	pub const fn downloads(self) -> bool {
		matches!(self, Self::Automatic)
	}
}

impl std::str::FromStr for LauncherUpdates {
	type Err = ();

	fn from_str(text: &str) -> Result<Self, ()> {
		Self::ALL
			.into_iter()
			.find(|mode| mode.as_str() == text.trim().to_ascii_lowercase())
			.ok_or(())
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
	pub launcher_updates: LauncherUpdates,
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
			launcher_updates: LauncherUpdates::default(),
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
		assert_eq!(defaults.launcher_updates, LauncherUpdates::Automatic);
	}

	#[test]
	fn automatic_follows_the_system_and_falls_back_to_english() {
		for (tag, expected) in [
			("ja-JP", Language::Japanese),
			("pt-PT", Language::PortugueseBrazil),
			("zh_Hans_CN", Language::ChineseSimplified),
			("de-AT", Language::German),
			("hi", Language::Hindi),
			("fr-FR", Language::English),
			("", Language::English),
		] {
			assert_eq!(Language::Auto.resolve(tag), expected, "{tag}");
		}

		assert_eq!(
			Language::German.resolve("ja-JP"),
			Language::German,
			"a fixed choice wins over the system"
		);
	}

	#[test]
	fn every_language_can_be_named_on_the_command_line_and_stored() {
		for language in Language::ALL {
			assert_eq!(language.as_str().replace('_', "-").parse(), Ok(language));

			let stored = serde_json::to_string(&language).unwrap();

			assert_eq!(serde_json::from_str::<Language>(&stored).unwrap(), language);
		}

		assert_eq!("pt-BR".parse(), Ok(Language::PortugueseBrazil));
		assert_eq!("zh-CN".parse(), Ok(Language::ChineseSimplified));
		assert_eq!("klingon".parse::<Language>(), Err(()));

		for language in Language::TRANSLATED {
			assert!(language.tag().is_some() && language.endonym().is_some());
		}
	}

	#[test]
	fn launcher_updates_have_their_own_names_and_meanings() {
		for mode in LauncherUpdates::ALL {
			assert_eq!(mode.as_str().parse(), Ok(mode));
		}

		assert_eq!("Tell".parse::<LauncherUpdates>(), Err(()));
		assert!(LauncherUpdates::Automatic.downloads() && LauncherUpdates::Automatic.checks());
		assert!(!LauncherUpdates::Notify.downloads() && LauncherUpdates::Notify.checks());
		assert!(!LauncherUpdates::Off.checks());
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
