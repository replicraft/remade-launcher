//! Times as the person reading them keeps time.
//!
//! The engine stores and exchanges UTC. Anything a person reads is shown in
//! their own time zone instead, so "published 3:42 PM" means their afternoon.
//! Machine output (JSON, signed records) stays UTC and does not use this.

use jiff::Timestamp;
use jiff::tz::TimeZone;

/// "Oct 24, 2026".
pub fn date(unix: i64) -> String {
	format_in(unix, &TimeZone::system(), "%b %-d, %Y")
}

/// "Oct 24, 2026, 3:42 PM PDT".
pub fn date_time(unix: i64) -> String {
	format_in(unix, &TimeZone::system(), "%b %-d, %Y, %-I:%M %p %Z")
}

/// "Today, 3:42 PM", "Yesterday, 9:05 AM", or the date for anything older.
pub fn friendly(unix: i64, now: i64) -> String {
	friendly_in(unix, now, &TimeZone::system())
}

fn friendly_in(unix: i64, now: i64, zone: &TimeZone) -> String {
	let (Ok(then), Ok(today)) = (Timestamp::from_second(unix), Timestamp::from_second(now)) else {
		return date_time(unix);
	};

	let then = then.to_zoned(zone.clone());
	let today = today.to_zoned(zone.clone());
	let clock = then.strftime("%-I:%M %p");
	let days_ago = today
		.date()
		.since(then.date())
		.map(|span| span.get_days())
		.unwrap_or(i32::MAX);

	match days_ago {
		0 => format!("Today, {clock}"),
		1 => format!("Yesterday, {clock}"),
		_ => then.strftime("%b %-d, %Y").to_string(),
	}
}

fn format_in(unix: i64, zone: &TimeZone, pattern: &str) -> String {
	match Timestamp::from_second(unix) {
		Ok(timestamp) => timestamp
			.to_zoned(zone.clone())
			.strftime(pattern)
			.to_string(),
		// A value outside the calendar is a bug upstream. Say so plainly.
		Err(_) => "an unknown time".into(),
	}
}

#[cfg(test)]
mod tests {
	use jiff::tz;

	use super::*;

	fn minus_five() -> TimeZone {
		TimeZone::fixed(tz::offset(-5))
	}

	#[test]
	fn the_same_instant_reads_differently_in_another_zone() {
		// 2026-10-24 20:42:00 UTC.
		let unix = 1_792_874_520;

		assert_eq!(
			format_in(unix, &TimeZone::UTC, "%b %-d, %Y, %-I:%M %p"),
			"Oct 24, 2026, 8:42 PM"
		);
		assert_eq!(
			format_in(unix, &minus_five(), "%b %-d, %Y, %-I:%M %p"),
			"Oct 24, 2026, 3:42 PM"
		);
	}

	#[test]
	fn an_early_morning_in_utc_is_still_the_day_before_locally() {
		// 2026-10-25 02:42:00 UTC.
		let unix = 1_792_874_520 + 6 * 3_600;

		assert_eq!(format_in(unix, &TimeZone::UTC, "%b %-d"), "Oct 25");
		assert_eq!(format_in(unix, &minus_five(), "%b %-d"), "Oct 24");
	}

	#[test]
	fn friendly_names_today_and_yesterday_by_local_calendar_days() {
		let now = 1_792_874_520;

		assert_eq!(friendly_in(now - 600, now, &minus_five()), "Today, 3:32 PM");
		assert_eq!(
			friendly_in(now - 86_400, now, &minus_five()),
			"Yesterday, 3:42 PM"
		);
		assert_eq!(
			friendly_in(now - 86_400 * 5, now, &minus_five()),
			"Oct 19, 2026"
		);
	}

	#[test]
	fn nonsense_never_panics() {
		assert_eq!(format_in(i64::MAX, &TimeZone::UTC, "%Y"), "an unknown time");
	}
}
