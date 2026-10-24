//! When the updater scans.
//!
//! These are pure functions of the saved times and the clock, so the rules
//! are testable without waiting 24 hours: one scan at activation unless one
//! just ran, then every 24 hours measured from the last completed scan, one
//! catch-up scan after sleep, bounded retries after a failure, and never a
//! scan per missed day.

/// The normal gap between scans.
pub const INTERVAL_SECONDS: i64 = 24 * 60 * 60;

/// A scan this recent is not repeated when the updater restarts.
pub const RECENT_SECONDS: i64 = 10 * 60;

/// The longest the updater sleeps between looks at the clock. A shorter
/// nap after a sleep means the catch-up scan is never far behind.
pub const MAX_NAP_SECONDS: i64 = 15 * 60;

const RETRY_BASE_SECONDS: i64 = 5 * 60;
const RETRY_CAP_SECONDS: i64 = 60 * 60;

/// How many times a failed scan is retried before waiting for the normal
/// schedule.
pub const MAX_RETRIES: u32 = 3;

/// What was saved by the last run.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SavedSchedule {
	pub last_attempted: Option<i64>,
	pub last_completed: Option<i64>,
	pub next_due: Option<i64>,
}

/// The time of the first scan after the updater starts.
pub fn first_due(saved: SavedSchedule, now: i64) -> i64 {
	match saved.last_attempted {
		// It just ran, perhaps before a crash or a restart. Do not scan twice.
		Some(attempted) if now - attempted < RECENT_SECONDS && now >= attempted => {
			saved.last_completed.unwrap_or(attempted) + INTERVAL_SECONDS
		}
		_ => now,
	}
}

/// The time of the next scan after one finished at `completed`. Always a
/// full interval later, whether the scan succeeded or not, and whatever the
/// number of missed intervals.
pub fn next_after(completed: i64) -> i64 {
	completed + INTERVAL_SECONDS
}

/// How long to wait before retrying a failed scan, or `None` once retries
/// are used up. Doubles each time, with a cap.
pub fn retry_delay(failures: u32) -> Option<i64> {
	if failures == 0 || failures > MAX_RETRIES {
		return None;
	}

	Some((RETRY_BASE_SECONDS << (failures - 1)).min(RETRY_CAP_SECONDS))
}

/// Whether the updater should scan now. After sleep the wall clock has
/// jumped past `next_due`, which yields one scan, not one per missed day.
pub fn is_due(next_due: i64, now: i64) -> bool {
	now >= next_due
}

/// How long to sleep before looking at the clock again.
pub fn nap(next_due: i64, now: i64) -> i64 {
	(next_due - now).clamp(1, MAX_NAP_SECONDS)
}

/// Whether the clock has moved backwards past what a scan recorded, which
/// would make "due" and "age" unreliable.
pub fn clock_went_backwards(last_attempted: Option<i64>, now: i64) -> bool {
	last_attempted.is_some_and(|attempted| now + 60 < attempted)
}

#[cfg(test)]
mod tests {
	use super::*;

	const NOW: i64 = 1_000_000;

	#[test]
	fn a_first_start_scans_immediately() {
		assert_eq!(first_due(SavedSchedule::default(), NOW), NOW);
	}

	#[test]
	fn a_restart_right_after_a_scan_does_not_scan_again() {
		let saved = SavedSchedule {
			last_attempted: Some(NOW - 120),
			last_completed: Some(NOW - 100),
			next_due: None,
		};

		assert_eq!(first_due(saved, NOW), NOW - 100 + INTERVAL_SECONDS);
	}

	#[test]
	fn an_old_scan_means_scan_now() {
		let saved = SavedSchedule {
			last_attempted: Some(NOW - 3_600),
			last_completed: Some(NOW - 3_600),
			next_due: None,
		};

		assert_eq!(first_due(saved, NOW), NOW);
	}

	#[test]
	fn a_scan_time_in_the_future_is_not_trusted() {
		let saved = SavedSchedule {
			last_attempted: Some(NOW + 500),
			last_completed: None,
			next_due: None,
		};

		assert_eq!(first_due(saved, NOW), NOW);
		assert!(clock_went_backwards(saved.last_attempted, NOW));
	}

	#[test]
	fn the_next_scan_is_a_full_interval_after_the_last_one_finished() {
		assert_eq!(next_after(NOW), NOW + 86_400);
	}

	#[test]
	fn sleeping_through_several_days_yields_one_catch_up_scan() {
		let due = NOW;
		let woke_up = NOW + 5 * INTERVAL_SECONDS;

		assert!(is_due(due, woke_up));

		// After that scan the next one is a day away, not four scans back to back.
		assert!(!is_due(next_after(woke_up), woke_up + 1));
	}

	#[test]
	fn retries_back_off_and_stop() {
		assert_eq!(retry_delay(0), None);
		assert_eq!(retry_delay(1), Some(300));
		assert_eq!(retry_delay(2), Some(600));
		assert_eq!(retry_delay(3), Some(1_200));
		assert_eq!(retry_delay(4), None, "then the normal schedule resumes");
	}

	#[test]
	fn the_updater_never_sleeps_past_the_nap_cap_and_never_spins() {
		assert_eq!(nap(NOW + 10 * 86_400, NOW), MAX_NAP_SECONDS);
		assert_eq!(nap(NOW + 30, NOW), 30);
		assert_eq!(nap(NOW - 5, NOW), 1);
	}
}
