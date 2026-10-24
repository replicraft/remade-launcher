//! Printing helpers.

use std::process::ExitCode;

use crate::context::Ctx;
use crate::errors::CliError;
use crate::exit::ExitStatus;

/// Reports a failure the way every command does, and returns its exit code.
///
/// In machine modes stdout carries exactly one JSON error object, and stderr
/// carries a short human line so a person watching a pipeline still sees it.
pub(crate) fn report(ctx: &Ctx, error: &CliError) -> ExitCode {
	if ctx.machine() {
		ctx.emit_error(error);
		eprintln!("error: {}", error.message);
	} else {
		eprintln!("error: {}", error.message);
		eprintln!("       ({})", error.code);
	}

	error.status.into()
}

/// A refusal that is not a fault in the launcher: nothing is wrong, but what
/// was asked cannot be done.
pub(crate) fn refuse(
	ctx: &Ctx,
	code: &'static str,
	status: ExitStatus,
	message: &str,
) -> ExitStatus {
	report(ctx, &CliError::new(code, status, message));

	status
}

/// Prints rows as a table with no borders, like most command-line tools.
///
/// On a terminal, long cells wrap to fit its width and headers are bold. When
/// the output is piped or redirected, nothing wraps and nothing is styled, so
/// each row stays on one plain line for `grep` and `awk`.
pub(crate) fn print_table(headers: &[&str], rows: &[Vec<String>]) {
	use comfy_table::presets::NOTHING;
	use comfy_table::{Attribute, Cell, ContentArrangement, Table};

	let interactive = std::io::IsTerminal::is_terminal(&std::io::stdout());
	let mut table = Table::new();

	table.load_style(NOTHING);

	if interactive {
		table.set_content_arrangement(ContentArrangement::Dynamic);
		table.set_header(
			headers
				.iter()
				.map(|header| Cell::new(header).add_attribute(Attribute::Bold)),
		);
	} else {
		table.set_content_arrangement(ContentArrangement::Disabled);
		table.force_no_tty();
		table.set_header(headers.iter().map(Cell::new));
	}

	for row in rows {
		table.add_row(row.iter().map(Cell::new));
	}

	// The preset leaves padding on each line. Trim it so output is flush left.
	for line in table.to_string().lines() {
		println!("{}", line.trim());
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

/// UTC, whole seconds, with a `Z`: the form every timestamp in JSON uses.
pub(crate) fn iso(unix: i64) -> String {
	let time =
		time::OffsetDateTime::from_unix_timestamp(unix).unwrap_or(time::OffsetDateTime::UNIX_EPOCH);

	format!(
		"{}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
		time.year(),
		u8::from(time.month()),
		time.day(),
		time.hour(),
		time.minute(),
		time.second()
	)
}

/// A timestamp for text output, with the time of day.
pub(crate) fn when(unix: i64) -> String {
	iso(unix).replace('T', " ").trim_end_matches('Z').to_owned() + " UTC"
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
	fn formats_sizes_and_times() {
		assert_eq!(bytes(1536), "1.5 KB");
		assert_eq!(date(1_894_665_600), "2030-01-15");
		assert_eq!(iso(1_894_665_600), "2030-01-15T00:00:00Z");
		assert_eq!(iso(1_894_669_445), "2030-01-15T01:04:05Z");
		assert_eq!(when(1_894_669_445), "2030-01-15 01:04:05 UTC");
	}
}
