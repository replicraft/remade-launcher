//! The command line keeping itself current, the way its setting says.
//!
//! Only a person at a terminal gets this. A script, a pipe, `--json`,
//! `--quiet`, CI, or `--offline` never sees a new version arrive in the
//! middle of a run: a command's behavior must not change between two steps of
//! a pipeline. Everything is bounded in time and failure is silent, so it can
//! never make a command fail or hang.

use std::io::IsTerminal;
use std::sync::Arc;
use std::time::Duration;

use launcher_core::{Engine, LauncherUpdateOutcome};
use launcher_model::LauncherUpdates;

use crate::context::{Ctx, Format};

/// Stops it from waiting on a slow network after the command is done.
const BUDGET: Duration = Duration::from_secs(25);

/// How long to leave an "a new version is available" note unrepeated.
const NOTE_INTERVAL_SECONDS: i64 = 24 * 60 * 60;

const NOTED_KEY: &str = "cli.launcher_update_noted";

/// Set to anything to turn this off, for example in a container image.
pub(crate) const OPT_OUT: &str = "REMADE_NO_SELF_UPDATE";

/// What the caller is, so whether to update can be decided in one place.
pub(crate) struct Situation {
	pub(crate) text_output: bool,
	pub(crate) quiet: bool,
	pub(crate) offline: bool,
	pub(crate) no_input: bool,
	pub(crate) terminal: bool,
	pub(crate) opted_out: bool,
	pub(crate) command: &'static str,
}

impl Situation {
	/// A person at a terminal, running something other than the commands that
	/// manage the launcher itself or run unattended.
	pub(crate) fn allows_update(&self) -> bool {
		self.text_output
			&& !self.quiet
			&& !self.offline
			&& !self.no_input
			&& self.terminal
			&& !self.opted_out
			&& !matches!(self.command, "self-update" | "completions" | "daemon")
	}
}

fn situation(ctx: &Ctx) -> Situation {
	Situation {
		text_output: ctx.format == Format::Text,
		quiet: ctx.quiet,
		offline: ctx.offline,
		no_input: ctx.no_input,
		terminal: std::io::stdin().is_terminal()
			&& std::io::stdout().is_terminal()
			&& std::io::stderr().is_terminal(),
		opted_out: std::env::var_os(OPT_OUT).is_some() || std::env::var_os("CI").is_some(),
		command: ctx.command_name(),
	}
}

/// Looks for a new launcher after a command succeeded, and downloads and
/// switches to it if the setting is Automatic.
pub(crate) async fn run(ctx: &Arc<Ctx>, engine: &Engine) {
	if !situation(ctx).allows_update() {
		return;
	}

	let mode = engine.preferences().await.launcher_updates;

	if !mode.checks() {
		return;
	}

	let _ = tokio::time::timeout(BUDGET, work(ctx, engine, mode)).await;
}

async fn work(ctx: &Ctx, engine: &Engine, mode: LauncherUpdates) {
	match engine.launcher_update_cycle().await {
		LauncherUpdateOutcome::Staged(_) if mode.downloads() => {
			match engine.apply_launcher_update().await {
				Ok(Some(applied)) => ctx.note(format!(
					"Updated remade from {} to {}. The new version runs the next time you use it.",
					applied.from, applied.to
				)),
				Ok(None) => {}
				Err(error) => tracing::info!(%error, "the new launcher could not be switched in"),
			}
		}
		LauncherUpdateOutcome::Staged(staged) => {
			remind(ctx, engine, &staged.version, "is downloaded").await;
		}
		LauncherUpdateOutcome::Available(release) => {
			remind(ctx, engine, &release.version, "is available").await;
		}
		LauncherUpdateOutcome::Blocked(_) | LauncherUpdateOutcome::Idle => {}
	}
}

/// Says a new version waits, at most once a day.
async fn remind(ctx: &Ctx, engine: &Engine, version: &str, what: &str) {
	let now = time::OffsetDateTime::now_utc().unix_timestamp();
	let recently = engine
		.state_value(NOTED_KEY)
		.await
		.and_then(|text| {
			let (seen, at) = text.split_once('@')?;

			Some((seen == version, at.parse::<i64>().ok()?))
		})
		.is_some_and(|(same, at)| same && now - at < NOTE_INTERVAL_SECONDS);

	if recently {
		return;
	}

	let _ = engine
		.set_state_value(NOTED_KEY, format!("{version}@{now}"))
		.await;

	ctx.note(format!(
		"remade {version} {what}. Run `remade self-update apply` to switch to it."
	));
}

#[cfg(test)]
mod tests {
	use super::*;

	fn person() -> Situation {
		Situation {
			text_output: true,
			quiet: false,
			offline: false,
			no_input: false,
			terminal: true,
			opted_out: false,
			command: "list",
		}
	}

	#[test]
	fn a_person_at_a_terminal_gets_updates() {
		assert!(person().allows_update());
	}

	#[test]
	fn scripts_pipes_and_machines_never_do() {
		let cases = [
			Situation {
				text_output: false,
				..person()
			},
			Situation {
				quiet: true,
				..person()
			},
			Situation {
				offline: true,
				..person()
			},
			Situation {
				no_input: true,
				..person()
			},
			Situation {
				terminal: false,
				..person()
			},
			Situation {
				opted_out: true,
				..person()
			},
		];

		for case in cases {
			assert!(!case.allows_update());
		}
	}

	#[test]
	fn the_commands_that_manage_the_launcher_are_left_alone() {
		for command in ["self-update", "completions", "daemon"] {
			assert!(
				!Situation {
					command,
					..person()
				}
				.allows_update()
			);
		}
	}
}
