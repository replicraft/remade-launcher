//! Watching apps the launcher did not install.
//!
//! These are observations, not installs. The launcher never removes or
//! replaces their files, so the commands that change files (`uninstall`,
//! `update`) say so instead of guessing.

use std::path::PathBuf;
use std::sync::Arc;

use launcher_core::{AdoptedApp, AdoptionRequest, Approval, Engine};
use serde_json::{Value, json};

use crate::cli::AdoptArgs;
use crate::context::Ctx;
use crate::errors::CliError;
use crate::exit::ExitStatus;
use crate::output::{bytes, iso, refuse};
use crate::prompt;

pub(crate) fn adopted_json(app: &AdoptedApp) -> Value {
	json!({
		"id": app.id.to_string(),
		"short_id": app.id.short(),
		"name": app.name,
		"folder": app.path.display().to_string(),
		"start_file": app.entry_point,
		"claimed_catalog_name": app.catalog_slug,
		"present": app.present,
		"adopted_at": iso(app.adopted_at),
		"last_seen_at": app.last_seen_at.map(iso),
		"managed_by_remade": false,
	})
}

/// The watched app a reference names, if it names one.
pub(crate) async fn find(
	engine: &Arc<Engine>,
	reference: &str,
) -> Result<Option<AdoptedApp>, CliError> {
	let apps = engine.adopted_apps().await?;

	Ok(crate::app_ref::find_adopted(&apps, reference)?.cloned())
}

pub(crate) async fn adopt(
	ctx: &Ctx,
	engine: &Arc<Engine>,
	args: &AdoptArgs,
) -> Result<ExitStatus, CliError> {
	let path = PathBuf::from(&args.path);
	let candidate = engine.inspect_for_adoption(&path).await?;

	if ctx.machine() && args.preview {
		ctx.emit(json!({
			"folder": candidate.path.display().to_string(),
			"suggested_name": candidate.suggested_name,
			"start_files": candidate.entry_points,
			"files": candidate.files,
			"bytes": candidate.bytes,
			"count_is_partial": candidate.truncated,
			"effect": "watch_only",
		}));

		return Ok(ExitStatus::Success);
	}

	if !ctx.machine() {
		println!("Folder:      {}", candidate.path.display());
		println!(
			"Contains:    {}{} files, {}",
			if candidate.truncated { "at least " } else { "" },
			candidate.files,
			bytes(candidate.bytes)
		);

		match candidate.entry_points.as_slice() {
			[] => println!("Start file:  none found. Pass --entry to choose one."),
			[only] => println!("Start file:  {only}"),
			several => println!("Start files: {}", several.join(", ")),
		}

		println!("What this does: the launcher will list this app and can start it.");
		println!("It will not update it, remove it, or change any of its files.");
	}

	if args.preview {
		return Ok(ExitStatus::Success);
	}

	if !prompt::confirm(ctx, "Watch this app?", args.yes)? {
		return Ok(refuse(
			ctx,
			"APPROVAL_REQUIRED",
			ExitStatus::ApprovalRequired,
			"Cancelled. Nothing was changed.",
		));
	}

	let adopted = engine
		.adopt(
			AdoptionRequest {
				path,
				name: args.name.clone(),
				catalog_slug: args.catalog_name.clone(),
				entry_point: args.entry.clone(),
			},
			&Approval {
				actor: "cli".into(),
				confirmed: true,
				accepted_unreviewed_source: None,
			},
		)
		.await?;

	if ctx.machine() {
		ctx.emit(adopted_json(&adopted));
	} else {
		println!("Now watching {} ({}).", adopted.name, adopted.id.short());
		println!(
			"Stop with `remade forget {}`. Its files stay where they are.",
			adopted.id.short()
		);
	}

	Ok(ExitStatus::Success)
}

/// Starts a watched app.
pub(crate) async fn launch(
	ctx: &Ctx,
	engine: &Arc<Engine>,
	app: &AdoptedApp,
) -> Result<ExitStatus, CliError> {
	engine.launch_adopted(app.id).await?;

	if ctx.machine() {
		ctx.emit(json!({ "app": app.name, "started": true }));
	} else {
		ctx.note(format!("Started {}.", app.name));
	}

	Ok(ExitStatus::Success)
}

/// Stops watching. The files are not touched.
pub(crate) async fn forget(
	ctx: &Ctx,
	engine: &Arc<Engine>,
	app: &AdoptedApp,
	yes: bool,
) -> Result<ExitStatus, CliError> {
	ctx.note(format!(
		"The launcher will stop watching {}. Its files stay where they are.",
		app.name
	));

	if !prompt::confirm(ctx, &format!("Stop watching {}?", app.name), yes)? {
		return Ok(refuse(
			ctx,
			"APPROVAL_REQUIRED",
			ExitStatus::ApprovalRequired,
			"Cancelled. Nothing was changed.",
		));
	}

	engine.forget_adopted(app.id).await?;

	if ctx.machine() {
		ctx.emit(json!({ "app": app.name, "forgotten": true, "files_left_in_place": app.path.display().to_string() }));
	} else {
		println!(
			"Stopped watching {}. Its files are still in {}.",
			app.name,
			app.path.display()
		);
	}

	Ok(ExitStatus::Success)
}

/// What `uninstall` and `update` say about a watched app.
pub(crate) fn not_managed(ctx: &Ctx, app: &AdoptedApp, what: &str) -> ExitStatus {
	refuse(
		ctx,
		"NOT_MANAGED",
		ExitStatus::Conflict,
		&format!(
			"{} is watched, not managed, so the launcher will not {what} it. Use `remade forget {}` to stop watching it.",
			app.name,
			app.id.short()
		),
	)
}

/// Whether a watched app's folder and start file are still there.
pub(crate) fn check(ctx: &Ctx, app: &AdoptedApp) -> ExitStatus {
	let entry_ok = app
		.entry_point
		.as_deref()
		.is_some_and(|entry| app.path.join(entry).exists());
	let ok = app.present && entry_ok;

	if ctx.machine() {
		ctx.emit(json!({
			"app": app.name,
			"watched": true,
			"folder_present": app.present,
			"start_file_present": entry_ok,
			"clean": ok,
		}));
	} else if ok {
		println!("{}: present (watched, not managed)", app.name);
	} else if !app.present {
		println!(
			"{}: the folder is not there right now (an unplugged drive is not a removal).",
			app.name
		);
	} else {
		println!("{}: the start file is missing.", app.name);
	}

	if ok {
		ExitStatus::Success
	} else {
		ExitStatus::Conflict
	}
}
