//! `remade self-update`: the launcher keeping itself current.
//!
//! This is the launcher's own update, apart from `remade updates`, which is
//! about installed apps. The setting that governs it is `launcher.updates`.

use std::sync::Arc;

use launcher_core::{Blocker, Engine, LauncherCheck, LauncherRelease, LauncherUpdateReport};
use launcher_model::product;
use serde_json::{Value, json};

use crate::cli::SelfUpdateAction;
use crate::context::Ctx;
use crate::errors::CliError;
use crate::exit::ExitStatus;
use crate::output::{bytes, refuse, when};
use crate::progress::DownloadProgress;
use crate::prompt;

const UNAVAILABLE: &str = "SELF_UPDATE_UNAVAILABLE";

fn release_json(release: &LauncherRelease) -> Value {
	json!({ "version": release.version, "notes_url": release.notes_url, "size": release.size })
}

fn report_json(report: &LauncherUpdateReport) -> Value {
	json!({
		"current_version": report.current,
		"mode": report.mode.as_str(),
		"checked_at": report.checked_at.map(crate::output::iso),
		"available": report.available.as_ref().map(release_json),
		"staged": report.staged.as_ref().map(|staged| json!({ "version": staged.version })),
		"applied": report.applied.as_ref().map(|applied| json!({
			"from": applied.from,
			"to": applied.to,
			"at": crate::output::iso(applied.at),
		})),
		"can_roll_back": report.can_roll_back,
		"unavailable_reason": report.blocker.as_ref().map(Blocker::message),
		// Kept for scripts written against the first version of this command.
		"available_to_install": report.blocker.is_none(),
	})
}

fn describe(release: &LauncherRelease) -> String {
	let size = release
		.size
		.map(|size| format!(" ({})", bytes(size)))
		.unwrap_or_default();

	format!("{} {}{size}", product::NAME, release.version)
}

pub(crate) async fn run(
	ctx: &Arc<Ctx>,
	engine: &Arc<Engine>,
	action: &SelfUpdateAction,
) -> Result<ExitStatus, CliError> {
	match action {
		SelfUpdateAction::Status => Ok(status(ctx, engine).await),
		SelfUpdateAction::Check => check(ctx, engine).await,
		SelfUpdateAction::Apply { yes } => apply(ctx, engine, *yes).await,
		SelfUpdateAction::Rollback { yes } => rollback(ctx, engine, *yes).await,
	}
}

async fn status(ctx: &Ctx, engine: &Engine) -> ExitStatus {
	let report = engine.launcher_update_report().await;

	if ctx.machine() {
		ctx.emit(report_json(&report));

		return ExitStatus::Success;
	}

	println!("{} {}", product::NAME, report.current);
	println!("Launcher updates: {}", report.mode.label());
	println!("  {}", report.mode.description());

	match report.checked_at {
		Some(at) => println!("Last checked: {}", when(at)),
		None => println!("Last checked: never"),
	}

	if let Some(staged) = &report.staged {
		println!(
			"{} is downloaded and ready. Run `remade self-update apply` to switch to it.",
			staged.version
		);
	} else if let Some(release) = &report.available {
		println!("{} is available.", describe(release));
	}

	if let Some(applied) = &report.applied {
		println!(
			"Last switched: {} to {} on {}",
			applied.from,
			applied.to,
			when(applied.at)
		);
	}

	if report.can_roll_back {
		println!("The previous launcher is kept. `remade self-update rollback` goes back to it.");
	}

	if let Some(blocker) = &report.blocker {
		println!("{}", blocker.message());
	}

	ExitStatus::Success
}

fn offline(ctx: &Ctx) -> ExitStatus {
	refuse(
		ctx,
		"OFFLINE",
		ExitStatus::Network,
		"Looking for a new launcher needs the network.",
	)
}

async fn check(ctx: &Ctx, engine: &Engine) -> Result<ExitStatus, CliError> {
	if ctx.offline {
		return Ok(offline(ctx));
	}

	let outcome = engine.check_launcher_update(false).await?;

	match outcome {
		LauncherCheck::Blocked(blocker) => Ok(refuse(
			ctx,
			UNAVAILABLE,
			ExitStatus::Unsupported,
			&blocker.message(),
		)),
		LauncherCheck::Off => Ok(ExitStatus::Success),
		LauncherCheck::UpToDate => {
			if ctx.machine() {
				ctx.emit(json!({ "current_version": product::VERSION, "update_available": false }));
			} else {
				println!("{} {} is the newest.", product::NAME, product::VERSION);
			}

			Ok(ExitStatus::Success)
		}
		LauncherCheck::Available(release) => {
			let report = engine.launcher_update_report().await;

			if ctx.machine() {
				ctx.emit(json!({
					"current_version": product::VERSION,
					"update_available": true,
					"release": release_json(&release),
					"unavailable_reason": report.blocker.as_ref().map(Blocker::message),
				}));
			} else {
				println!(
					"{} is available (you have {}).",
					describe(&release),
					product::VERSION
				);

				if let Some(url) = &release.notes_url {
					println!("Notes: {url}");
				}

				match &report.blocker {
					Some(blocker) => println!("{}", blocker.message()),
					None => println!("Run `remade self-update apply` to switch to it."),
				}
			}

			Ok(ExitStatus::Success)
		}
	}
}

async fn apply(ctx: &Arc<Ctx>, engine: &Engine, yes: bool) -> Result<ExitStatus, CliError> {
	let report = engine.launcher_update_report().await;

	if let Some(blocker) = &report.blocker {
		return Ok(refuse(
			ctx,
			UNAVAILABLE,
			ExitStatus::Unsupported,
			&blocker.message(),
		));
	}

	// A launcher downloaded earlier needs no second download.
	let version = match report.staged {
		Some(staged) => staged.version,
		None if ctx.offline => return Ok(offline(ctx)),
		None => match engine.check_launcher_update(false).await? {
			LauncherCheck::Available(release) => {
				if !prompt::confirm(
					ctx,
					&format!("Download and switch to {}?", describe(&release)),
					yes,
				)? {
					return Ok(ExitStatus::ApprovalRequired);
				}

				let download = DownloadProgress::start(ctx);
				let mut progress = download.callback();
				let staged = engine.stage_launcher_update(&mut progress).await;

				download.finish();

				staged?.version
			}
			LauncherCheck::Blocked(blocker) => {
				return Ok(refuse(
					ctx,
					UNAVAILABLE,
					ExitStatus::Unsupported,
					&blocker.message(),
				));
			}
			LauncherCheck::UpToDate | LauncherCheck::Off => {
				if ctx.machine() {
					ctx.emit(json!({ "updated": false, "current_version": product::VERSION }));
				} else {
					println!("{} {} is the newest.", product::NAME, product::VERSION);
				}

				return Ok(ExitStatus::Success);
			}
		},
	};

	let Some(applied) = engine.apply_launcher_update().await? else {
		return Ok(refuse(
			ctx,
			UNAVAILABLE,
			ExitStatus::Unsupported,
			"There is nothing to switch to.",
		));
	};

	if ctx.machine() {
		ctx.emit(
			json!({ "updated": true, "from": applied.from, "to": applied.to, "staged_version": version }),
		);
	} else {
		println!("Switched from {} to {}.", applied.from, applied.to);
		println!("The next time you run it, you get the new version.");
	}

	Ok(ExitStatus::Success)
}

async fn rollback(ctx: &Ctx, engine: &Engine, yes: bool) -> Result<ExitStatus, CliError> {
	let report = engine.launcher_update_report().await;

	if !report.can_roll_back {
		return Ok(refuse(
			ctx,
			UNAVAILABLE,
			ExitStatus::Unsupported,
			"There is no earlier launcher to go back to.",
		));
	}

	if !prompt::confirm(ctx, "Go back to the previous launcher?", yes)? {
		return Ok(ExitStatus::ApprovalRequired);
	}

	let applied = engine.roll_back_launcher().await?;

	if ctx.machine() {
		ctx.emit(json!({ "rolled_back": true, "from": applied.from, "to": applied.to }));
	} else {
		println!("Went back from {} to {}.", applied.from, applied.to);
	}

	Ok(ExitStatus::Success)
}
