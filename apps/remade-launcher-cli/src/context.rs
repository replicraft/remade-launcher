//! What every command shares: the output format, the quiet and no-input
//! switches, and a place to collect warnings.
//!
//! Output format is formatting, never permission: `--json` changes how
//! results look, not what a command is allowed to do or what it asks first.

use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::{Value, json};

use crate::errors::CliError;
use crate::output::iso;

/// The version of the JSON shapes. Bump it when one changes incompatibly.
pub(crate) const SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Format {
	Text,
	Json,
	Ndjson,
}

pub(crate) struct Ctx {
	pub(crate) format: Format,
	pub(crate) quiet: bool,
	pub(crate) no_progress: bool,
	pub(crate) no_input: bool,
	pub(crate) offline: bool,
	command: &'static str,
	warnings: Mutex<Vec<String>>,
	sequence: AtomicU64,
}

impl Ctx {
	pub(crate) fn new(
		format: Format,
		quiet: bool,
		no_progress: bool,
		no_input: bool,
		offline: bool,
		command: &'static str,
	) -> Self {
		Self {
			format,
			quiet,
			no_progress,
			no_input,
			offline,
			command,
			warnings: Mutex::new(Vec::new()),
			sequence: AtomicU64::new(0),
		}
	}

	/// True for `--json` and `--output ndjson`, where stdout is for programs.
	pub(crate) fn machine(&self) -> bool {
		self.format != Format::Text
	}

	/// A warning. Machines get it in the result's `warnings`; people get it
	/// on stderr.
	pub(crate) fn warn(&self, text: impl Into<String>) {
		let text = text.into();

		if self.machine() {
			if let Ok(mut warnings) = self.warnings.lock() {
				warnings.push(text);
			}
		} else {
			eprintln!("warning: {text}");
		}
	}

	/// Something worth saying that is not a warning. Silenced by `--quiet`
	/// and in machine output.
	pub(crate) fn note(&self, text: impl AsRef<str>) {
		if !self.machine() && !self.quiet {
			eprintln!("{}", text.as_ref());
		}
	}

	fn warnings(&self) -> Vec<String> {
		self.warnings
			.lock()
			.map(|warnings| warnings.clone())
			.unwrap_or_default()
	}

	/// Prints the one JSON object a successful command produces.
	pub(crate) fn emit(&self, data: Value) {
		let envelope = json!({
			"schema_version": SCHEMA_VERSION,
			"command": self.command,
			"ok": true,
			"data": data,
			"warnings": self.warnings(),
		});

		println!("{envelope}");
	}

	/// Prints the one JSON object a failed command produces.
	pub(crate) fn emit_error(&self, error: &CliError) {
		let envelope = json!({
			"schema_version": SCHEMA_VERSION,
			"command": self.command,
			"ok": false,
			"error": {
				"code": error.code,
				"message": error.message,
				"retryable": error.retryable,
				"exit_status": error.status.code(),
			},
			"warnings": self.warnings(),
		});

		println!("{envelope}");
	}

	/// One progress or final event, for `--output ndjson`. Each line is a
	/// complete object with a sequence number and a UTC time.
	pub(crate) fn event(&self, mut event: Value) {
		if self.format != Format::Ndjson {
			return;
		}

		if let Some(object) = event.as_object_mut() {
			object.insert("schema_version".into(), json!(SCHEMA_VERSION));
			object.insert(
				"sequence".into(),
				json!(self.sequence.fetch_add(1, Ordering::Relaxed) + 1),
			);
			object.insert(
				"time".into(),
				json!(iso(time::OffsetDateTime::now_utc().unix_timestamp())),
			);
		}

		println!("{event}");
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	fn ctx(format: Format) -> Ctx {
		Ctx::new(format, false, false, false, false, "test")
	}

	#[test]
	fn only_text_is_for_people() {
		assert!(!ctx(Format::Text).machine());
		assert!(ctx(Format::Json).machine());
		assert!(ctx(Format::Ndjson).machine());
	}

	#[test]
	fn machine_warnings_are_collected_not_printed() {
		let ctx = ctx(Format::Json);

		ctx.warn("one");
		ctx.warn("two");

		assert_eq!(ctx.warnings(), vec!["one".to_owned(), "two".to_owned()]);
	}

	#[test]
	fn text_mode_collects_nothing() {
		let ctx = ctx(Format::Text);

		ctx.warn("shown on stderr instead");

		assert!(ctx.warnings().is_empty());
	}
}
