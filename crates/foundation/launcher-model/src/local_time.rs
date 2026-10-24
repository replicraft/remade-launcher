//! Times as the person reading them keeps time.
//!
//! The engine stores and exchanges UTC. Anything a person reads is shown in
//! their own time zone instead, so "published 3:42 PM" means their afternoon.
//! The clock (12 or 24 hour) and the order of day, month, and year follow the
//! system's language and region, unless the person chose otherwise in
//! Settings. Machine output (JSON, signed records) stays UTC and does not use
//! this.

use std::sync::{OnceLock, RwLock};

use jiff::Timestamp;
use jiff::tz::TimeZone;
use serde::{Deserialize, Serialize};

use crate::preferences::Language;

/// Which clock to show.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TimeFormat {
	/// What the system's region uses.
	#[default]
	System,
	Hour12,
	Hour24,
}

impl TimeFormat {
	pub const ALL: [Self; 3] = [Self::System, Self::Hour12, Self::Hour24];

	/// The name used on the command line and to pick a translated label.
	pub const fn as_str(self) -> &'static str {
		match self {
			Self::System => "system",
			Self::Hour12 => "12h",
			Self::Hour24 => "24h",
		}
	}

	pub const fn label(self) -> &'static str {
		match self {
			Self::System => "System",
			Self::Hour12 => "12-hour (3:42 PM)",
			Self::Hour24 => "24-hour (15:42)",
		}
	}
}

/// How to order and write a date.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DateFormat {
	/// What the system's region uses.
	#[default]
	System,
	/// Oct 24, 2026
	MonthDayYear,
	/// 24 Oct 2026
	DayMonthYear,
	/// 2026-10-24
	Iso,
}

impl DateFormat {
	pub const ALL: [Self; 4] = [
		Self::System,
		Self::MonthDayYear,
		Self::DayMonthYear,
		Self::Iso,
	];

	/// The name used on the command line and to pick a translated label.
	pub const fn as_str(self) -> &'static str {
		match self {
			Self::System => "system",
			Self::MonthDayYear => "mdy",
			Self::DayMonthYear => "dmy",
			Self::Iso => "iso",
		}
	}

	pub const fn label(self) -> &'static str {
		match self {
			Self::System => "System",
			Self::MonthDayYear => "Oct 24, 2026",
			Self::DayMonthYear => "24 Oct 2026",
			Self::Iso => "2026-10-24",
		}
	}
}

/// What a locale tag implies, before any override.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Conventions {
	pub twelve_hour: bool,
	pub dates: DateStyle,
}

/// A concrete date order. `System` is resolved to one of these.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DateStyle {
	MonthDayYear,
	DayMonthYear,
	Iso,
}

/// The usual clock and date order for a locale tag such as `en-US` or
/// `de_DE.UTF-8`. This is a short table of the common cases, not a locale
/// database: anything it does not know gets a 24-hour clock and day first.
pub fn conventions(tag: &str) -> Conventions {
	let normalized = tag
		.split(['.', '@'])
		.next()
		.unwrap_or(tag)
		.replace('_', "-");
	let mut parts = normalized.split('-');
	let language = parts.next().unwrap_or("").to_ascii_lowercase();
	let region = parts
		.find(|part| part.len() == 2 && part.chars().all(|c| c.is_ascii_alphabetic()))
		.map(str::to_ascii_uppercase)
		.unwrap_or_default();

	let twelve_hour = match (language.as_str(), region.as_str()) {
		// English is split by region; the rest of these are the regions
		// that keep a 12-hour clock in everyday use.
		("en", "" | "US" | "CA" | "AU" | "NZ" | "IN" | "PH" | "PK" | "BD" | "MY" | "SG") => true,
		("es", "US" | "MX" | "CO" | "VE")
		| ("hi" | "ar" | "bn" | "ur" | "ko" | "fil" | "tl", _) => true,
		_ => false,
	};

	let dates = match (language.as_str(), region.as_str()) {
		("en", "" | "US" | "PH") => DateStyle::MonthDayYear,
		("ja" | "zh" | "ko" | "hu" | "lt" | "mn", _) | ("sv", "SE") | ("en", "CA") => {
			DateStyle::Iso
		}
		_ => DateStyle::DayMonthYear,
	};

	Conventions { twelve_hour, dates }
}

/// The system's locale tag, read once.
fn system_tag() -> &'static str {
	static TAG: OnceLock<String> = OnceLock::new();

	TAG.get_or_init(|| sys_locale::get_locale().unwrap_or_else(|| "en-US".to_owned()))
}

/// The tag the system reports, such as `en-US`, for showing in Settings.
pub fn system_locale() -> &'static str {
	system_tag()
}

#[derive(Clone, Copy)]
struct Chosen {
	time: TimeFormat,
	date: DateFormat,
}

static CHOSEN: RwLock<Chosen> = RwLock::new(Chosen {
	time: TimeFormat::System,
	date: DateFormat::System,
});

/// Sets the person's choices. Everything formatted afterwards follows them.
pub fn configure(time: TimeFormat, date: DateFormat) {
	if let Ok(mut chosen) = CHOSEN.write() {
		*chosen = Chosen { time, date };
	}
}

static LANGUAGE: RwLock<Language> = RwLock::new(Language::English);

/// Sets the language dates and clocks are written in. `Auto` must already be
/// resolved to a language.
pub fn set_language(language: Language) {
	if let Ok(mut current) = LANGUAGE.write() {
		*current = language;
	}
}

fn language() -> Language {
	LANGUAGE
		.read()
		.map_or(Language::English, |language| *language)
}

fn chosen() -> Chosen {
	CHOSEN.read().map_or(
		Chosen {
			time: TimeFormat::System,
			date: DateFormat::System,
		},
		|chosen| *chosen,
	)
}

/// Resolves the choices against `tag`'s conventions.
fn resolve(time: TimeFormat, date: DateFormat, tag: &str) -> Conventions {
	let system = conventions(tag);

	Conventions {
		twelve_hour: match time {
			TimeFormat::System => system.twelve_hour,
			TimeFormat::Hour12 => true,
			TimeFormat::Hour24 => false,
		},
		dates: match date {
			DateFormat::System => system.dates,
			DateFormat::MonthDayYear => DateStyle::MonthDayYear,
			DateFormat::DayMonthYear => DateStyle::DayMonthYear,
			DateFormat::Iso => DateStyle::Iso,
		},
	}
}

fn current() -> Conventions {
	let chosen = chosen();

	resolve(chosen.time, chosen.date, system_tag())
}

/// The words and order a language writes dates and clocks with.
struct Words {
	months: [&'static str; 12],
	today: &'static str,
	yesterday: &'static str,
}

fn words(language: Language) -> Words {
	match language {
		Language::German => Words {
			months: [
				"Jan.",
				"Feb.",
				"M\u{e4}rz",
				"Apr.",
				"Mai",
				"Juni",
				"Juli",
				"Aug.",
				"Sept.",
				"Okt.",
				"Nov.",
				"Dez.",
			],
			today: "Heute",
			yesterday: "Gestern",
		},
		Language::Spanish => Words {
			months: [
				"ene", "feb", "mar", "abr", "may", "jun", "jul", "ago", "sept", "oct", "nov", "dic",
			],
			today: "Hoy",
			yesterday: "Ayer",
		},
		Language::PortugueseBrazil => Words {
			months: [
				"jan.", "fev.", "mar.", "abr.", "mai.", "jun.", "jul.", "ago.", "set.", "out.",
				"nov.", "dez.",
			],
			today: "Hoje",
			yesterday: "Ontem",
		},
		Language::Hindi => Words {
			months: [
				"जन॰",
				"फ़र॰",
				"मार्च",
				"अप्रैल",
				"मई",
				"जून",
				"जुल॰",
				"अग॰",
				"सित॰",
				"अक्टू॰",
				"नव॰",
				"दिस॰",
			],
			today: "आज",
			yesterday: "बीता कल",
		},
		Language::Japanese => Words {
			months: [""; 12],
			today: "今日",
			yesterday: "昨日",
		},
		Language::ChineseSimplified => Words {
			months: [""; 12],
			today: "今天",
			yesterday: "昨天",
		},
		Language::English | Language::Auto => Words {
			months: [
				"Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
			],
			today: "Today",
			yesterday: "Yesterday",
		},
	}
}

/// Japanese and Chinese write the year, month, and day with their own
/// characters, in that order, whatever the "order" setting says.
fn writes_year_month_day(language: Language) -> bool {
	matches!(language, Language::Japanese | Language::ChineseSimplified)
}

fn date_text(when: &jiff::Zoned, style: DateStyle, language: Language) -> String {
	let (year, month, day) = (when.year(), when.month(), when.day());

	if style == DateStyle::Iso {
		return format!("{year}-{month:02}-{day:02}");
	}

	if writes_year_month_day(language) {
		return format!("{year}年{month}月{day}日");
	}

	let name = words(language).months[usize::from(month.unsigned_abs()).saturating_sub(1) % 12];

	match (style, language) {
		(DateStyle::DayMonthYear, Language::German) => format!("{day}. {name} {year}"),
		(DateStyle::DayMonthYear, Language::PortugueseBrazil) => {
			format!("{day} de {name} de {year}")
		}
		(DateStyle::DayMonthYear, _) => format!("{day} {name} {year}"),
		_ => format!("{name} {day}, {year}"),
	}
}

fn clock_text(when: &jiff::Zoned, twelve_hour: bool, language: Language) -> String {
	let (hour, minute) = (when.hour(), when.minute());

	if !twelve_hour {
		return format!("{hour:02}:{minute:02}");
	}

	let shown = if hour % 12 == 0 { 12 } else { hour % 12 };
	let afternoon = hour >= 12;

	match language {
		Language::Japanese => format!(
			"{}{shown}:{minute:02}",
			if afternoon { "午後" } else { "午前" }
		),
		Language::ChineseSimplified => {
			format!(
				"{}{shown}:{minute:02}",
				if afternoon { "下午" } else { "上午" }
			)
		}
		Language::Hindi => format!(
			"{shown}:{minute:02} {}",
			if afternoon {
				"अपराह्न"
			} else {
				"पूर्वाह्न"
			}
		),
		Language::Spanish => format!(
			"{shown}:{minute:02} {}",
			if afternoon { "p. m." } else { "a. m." }
		),
		_ => format!(
			"{shown}:{minute:02} {}",
			if afternoon { "PM" } else { "AM" }
		),
	}
}

fn zoned(unix: i64, zone: &TimeZone) -> Option<jiff::Zoned> {
	Timestamp::from_second(unix)
		.ok()
		.map(|timestamp| timestamp.to_zoned(zone.clone()))
}

/// "Oct 24, 2026", "24 Oct 2026", or "2026-10-24", in the interface language.
pub fn date(unix: i64) -> String {
	let Some(when) = zoned(unix, &TimeZone::system()) else {
		return UNKNOWN.into();
	};

	date_text(&when, current().dates, language())
}

/// A date and time with the zone, such as "Oct 24, 2026, 3:42 PM PDT".
pub fn date_time(unix: i64) -> String {
	date_time_in(unix, &TimeZone::system(), current(), language())
}

fn date_time_in(
	unix: i64,
	zone: &TimeZone,
	conventions: Conventions,
	language: Language,
) -> String {
	let Some(when) = zoned(unix, zone) else {
		return UNKNOWN.into();
	};
	let separator = if conventions.dates == DateStyle::Iso || writes_year_month_day(language) {
		" "
	} else {
		", "
	};

	format!(
		"{}{separator}{} {}",
		date_text(&when, conventions.dates, language),
		clock_text(&when, conventions.twelve_hour, language),
		when.strftime("%Z")
	)
}

/// "Today, 3:42 PM", "Yesterday, 9:05 AM", or the date for anything older.
pub fn friendly(unix: i64, now: i64) -> String {
	friendly_in(unix, now, &TimeZone::system(), current(), language())
}

fn friendly_in(
	unix: i64,
	now: i64,
	zone: &TimeZone,
	conventions: Conventions,
	language: Language,
) -> String {
	let (Some(then), Some(today)) = (zoned(unix, zone), zoned(now, zone)) else {
		return date_time(unix);
	};

	let clock = clock_text(&then, conventions.twelve_hour, language);
	let separator = if writes_year_month_day(language) {
		" "
	} else {
		", "
	};
	let days_ago = today
		.date()
		.since(then.date())
		.map(|span| span.get_days())
		.unwrap_or(i32::MAX);

	match days_ago {
		0 => format!("{}{separator}{clock}", words(language).today),
		1 => format!("{}{separator}{clock}", words(language).yesterday),
		_ => date_text(&then, conventions.dates, language),
	}
}

/// A value outside the calendar is a bug upstream. Say so plainly.
const UNKNOWN: &str = "an unknown time";

#[cfg(test)]
fn format_in(unix: i64, zone: &TimeZone, pattern: &str) -> String {
	zoned(unix, zone).map_or_else(|| UNKNOWN.into(), |when| when.strftime(pattern).to_string())
}

#[cfg(test)]
mod tests {
	use jiff::tz;

	use super::*;

	const NOW: i64 = 1_792_874_520; // 2026-10-24 20:42:00 UTC

	fn minus_five() -> TimeZone {
		TimeZone::fixed(tz::offset(-5))
	}

	fn us() -> Conventions {
		resolve(TimeFormat::System, DateFormat::System, "en-US")
	}

	#[test]
	fn the_same_instant_reads_differently_in_another_zone() {
		assert_eq!(
			format_in(NOW, &TimeZone::UTC, "%b %-d, %Y, %-I:%M %p"),
			"Oct 24, 2026, 8:42 PM"
		);
		assert_eq!(
			format_in(NOW, &minus_five(), "%b %-d, %Y, %-I:%M %p"),
			"Oct 24, 2026, 3:42 PM"
		);
	}

	#[test]
	fn an_early_morning_in_utc_is_still_the_day_before_locally() {
		let unix = NOW + 6 * 3_600;

		assert_eq!(format_in(unix, &TimeZone::UTC, "%b %-d"), "Oct 25");
		assert_eq!(format_in(unix, &minus_five(), "%b %-d"), "Oct 24");
	}

	#[test]
	fn friendly_names_today_and_yesterday_by_local_calendar_days() {
		assert_eq!(
			friendly_in(NOW - 600, NOW, &minus_five(), us(), Language::English),
			"Today, 3:32 PM"
		);
		assert_eq!(
			friendly_in(NOW - 86_400, NOW, &minus_five(), us(), Language::English),
			"Yesterday, 3:42 PM"
		);
		assert_eq!(
			friendly_in(
				NOW - 86_400 * 5,
				NOW,
				&minus_five(),
				us(),
				Language::English
			),
			"Oct 19, 2026"
		);
	}

	#[test]
	fn nonsense_never_panics() {
		assert_eq!(format_in(i64::MAX, &TimeZone::UTC, "%Y"), "an unknown time");
	}

	#[test]
	fn regions_get_their_usual_clock_and_date_order() {
		assert_eq!(
			conventions("en-US"),
			Conventions {
				twelve_hour: true,
				dates: DateStyle::MonthDayYear
			}
		);
		assert_eq!(
			conventions("en_GB.UTF-8"),
			Conventions {
				twelve_hour: false,
				dates: DateStyle::DayMonthYear
			}
		);
		assert_eq!(
			conventions("de-DE"),
			Conventions {
				twelve_hour: false,
				dates: DateStyle::DayMonthYear
			}
		);
		assert_eq!(
			conventions("ja-JP"),
			Conventions {
				twelve_hour: false,
				dates: DateStyle::Iso
			}
		);
		assert_eq!(
			conventions("en-AU"),
			Conventions {
				twelve_hour: true,
				dates: DateStyle::DayMonthYear
			}
		);
		assert_eq!(conventions("sv-SE").dates, DateStyle::Iso);
		// No region and no language we know: the safe, unambiguous defaults.
		assert_eq!(
			conventions("xx"),
			Conventions {
				twelve_hour: false,
				dates: DateStyle::DayMonthYear
			}
		);
	}

	#[test]
	fn an_override_beats_the_system_and_system_follows_it() {
		let forced = resolve(TimeFormat::Hour24, DateFormat::Iso, "en-US");

		assert!(!forced.twelve_hour);
		assert_eq!(forced.dates, DateStyle::Iso);
		assert_eq!(
			date_time_in(NOW, &minus_five(), forced, Language::English),
			"2026-10-24 15:42 -05"
		);

		let german = resolve(TimeFormat::System, DateFormat::System, "de-DE");

		assert_eq!(
			date_time_in(NOW, &minus_five(), german, Language::English),
			"24 Oct 2026, 15:42 -05"
		);
		assert_eq!(
			friendly_in(NOW - 600, NOW, &minus_five(), german, Language::English),
			"Today, 15:32"
		);
	}

	#[test]
	fn spanish_dates_and_clocks() {
		assert_eq!(
			date_time_in(NOW, &minus_five(), us(), Language::Spanish),
			"oct 24, 2026, 3:42 p. m. -05"
		);
		assert_eq!(
			friendly_in(NOW - 600, NOW, &minus_five(), us(), Language::Spanish),
			"Hoy, 3:32 p. m."
		);
	}

	#[test]
	fn dates_and_clocks_are_written_the_way_each_language_writes_them() {
		let us = us();
		let european = resolve(TimeFormat::System, DateFormat::System, "de-DE");
		let day = |conventions, language| date_time_in(NOW, &minus_five(), conventions, language);

		assert_eq!(day(us, Language::German), "Okt. 24, 2026, 3:42 PM -05");
		assert_eq!(day(european, Language::German), "24. Okt. 2026, 15:42 -05");
		assert_eq!(
			day(european, Language::PortugueseBrazil),
			"24 de out. de 2026, 15:42 -05"
		);
		assert_eq!(day(us, Language::Japanese), "2026年10月24日 午後3:42 -05");
		assert_eq!(
			day(european, Language::ChineseSimplified),
			"2026年10月24日 15:42 -05"
		);
		assert_eq!(day(us, Language::Hindi), "अक्टू॰ 24, 2026, 3:42 अपराह्न -05");
		assert_eq!(
			friendly_in(NOW - 600, NOW, &minus_five(), european, Language::German),
			"Heute, 15:32"
		);
		assert_eq!(
			friendly_in(NOW - 86_400, NOW, &minus_five(), us, Language::Japanese),
			"昨日 午後3:42"
		);
		assert_eq!(
			friendly_in(
				NOW - 600,
				NOW,
				&minus_five(),
				us,
				Language::PortugueseBrazil
			),
			"Hoje, 3:32 PM"
		);
	}
}
