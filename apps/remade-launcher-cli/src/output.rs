//! Printing: errors, tables, and JSON.

use std::process::ExitCode;

use launcher_model::LauncherError;
use serde::Serialize;

/// Prints a failure the way every command reports one, and returns the exit
/// code to use.
pub(crate) fn failure(error: &LauncherError) -> ExitCode {
	eprintln!("error: {}", error.user_message());
	eprintln!("       ({})", error.code().as_str());

	ExitCode::FAILURE
}

/// A refusal that is not an error in the launcher: nothing is wrong, but
/// what was asked cannot be done.
pub(crate) fn refuse(message: &str) -> ExitCode {
	eprintln!("{message}");

	ExitCode::FAILURE
}

pub(crate) fn print_json<T: Serialize>(value: &T) {
	match serde_json::to_string_pretty(value) {
		Ok(text) => println!("{text}"),
		Err(error) => eprintln!("error: could not format the result: {error}"),
	}
}

/// Prints rows in aligned columns. The last column is not padded.
pub(crate) fn print_table(headers: &[&str], rows: &[Vec<String>]) {
	let mut widths: Vec<usize> = headers
		.iter()
		.map(|header| header.chars().count())
		.collect();

	for row in rows {
		for (width, cell) in widths.iter_mut().zip(row) {
			*width = (*width).max(cell.chars().count());
		}
	}

	let line = |cells: Vec<&str>| {
		let last = cells.len().saturating_sub(1);
		let padded: Vec<String> = cells
			.iter()
			.enumerate()
			.map(|(index, cell)| {
				if index == last {
					(*cell).to_owned()
				} else {
					format!("{cell:<width$}", width = widths[index])
				}
			})
			.collect();

		println!("{}", padded.join("  "));
	};

	line(headers.to_vec());

	for row in rows {
		line(row.iter().map(String::as_str).collect());
	}
}

/// Shortens `text` to `max` characters, ending with an ellipsis.
pub(crate) fn truncate(text: &str, max: usize) -> String {
	if text.chars().count() <= max {
		return text.to_owned();
	}

	let kept: String = text.chars().take(max.saturating_sub(1)).collect();

	format!("{}…", kept.trim_end())
}

pub(crate) fn bytes(size: u64) -> String {
	const UNITS: [&str; 4] = ["B", "KB", "MB", "GB"];

	let mut value = size as f64;
	let mut unit = 0;

	while value >= 1024.0 && unit < UNITS.len() - 1 {
		value /= 1024.0;
		unit += 1;
	}

	if unit == 0 {
		format!("{size} B")
	} else {
		format!("{value:.1} {}", UNITS[unit])
	}
}

pub(crate) fn date(unix: i64) -> String {
	let time =
		time::OffsetDateTime::from_unix_timestamp(unix).unwrap_or(time::OffsetDateTime::UNIX_EPOCH);

	format!(
		"{}-{:02}-{:02}",
		time.year(),
		u8::from(time.month()),
		time.day()
	)
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn truncates_on_characters_not_bytes() {
		assert_eq!(truncate("short", 10), "short");
		assert_eq!(truncate("héllo wörld, long text", 8), "héllo w…");
		assert_eq!(truncate("exactly10!", 10), "exactly10!");
	}

	#[test]
	fn formats_sizes_and_dates() {
		assert_eq!(bytes(1536), "1.5 KB");
		assert_eq!(date(1_894_665_600), "2030-01-15");
	}
}
