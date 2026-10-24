//! Commands about what is installed: list, check, outdated, history,
//! operations, verify, doctor, path, and pins.

use std::sync::Arc;

use launcher_core::{Engine, UpdateState, UpdateStatus};
use serde_json::{Value, json};

use super::shared::{app_json, ensure_catalog, find_app, status_json, status_line};
use crate::cli::{
	AppArgs, CheckArgs, HistoryArgs, OperationsAction, OptionalAppArgs, OutdatedArgs,
};
use crate::context::Ctx;
use crate::errors::CliError;
use crate::exit::ExitStatus;
use crate::output::{iso, print_table, refuse, truncate, when};

pub(crate) async fn list(ctx: &Ctx, engine: &Arc<Engine>) -> Result<ExitStatus, CliError> {
	let apps = engine.installed().await?;

	if ctx.machine() {
		ctx.emit(json!({
			"managed": apps.iter().map(app_json).collect::<Vec<_>>(),
			// Apps installed by other tools are not looked for yet, so this is
			// honestly empty rather than missing.
			"externally_managed": [],
			"externally_managed_detection": "not_available",
		}));

		return Ok(ExitStatus::Success);
	}

	if apps.is_empty() {
		ctx.note("Nothing is installed. Try `remade search`.");

		return Ok(ExitStatus::Success);
	}

	let rows: Vec<Vec<String>> = apps
		.iter()
		.map(|app| {
			vec![
				app.id.short(),
				app.name.clone(),
				app.version.clone(),
				app.repository.clone(),
				if app.curated {
					"reviewed"
				} else {
					"added by you"
				}
				.to_owned(),
			]
		})
		.collect();

	print_table(&["ID", "NAME", "VERSION", "SOURCE", "ORIGIN"], &rows);

	Ok(ExitStatus::Success)
}

/// Looks for updates. `only_changes` hides apps that are up to date.
async fn look(
	ctx: &Ctx,
	engine: &Arc<Engine>,
	app: Option<&str>,
	only_changes: bool,
) -> Result<ExitStatus, CliError> {
	if ctx.offline {
		return Ok(refuse(
			ctx,
			"OFFLINE",
			ExitStatus::Network,
			"Looking for updates needs the network.",
		));
	}

	ensure_catalog(ctx, engine).await?;

	let apps = engine.installed().await?;
	let only = match app {
		Some(reference) => Some(find_app(&apps, reference)?.id),
		None => None,
	};

	let statuses = engine.check_updates(only).await?;
	let shown: Vec<&UpdateStatus> = statuses
		.iter()
		.filter(|status| !only_changes || !matches!(status.state, UpdateState::UpToDate))
		.collect();

	let failed = statuses
		.iter()
		.any(|status| matches!(status.state, UpdateState::CheckFailed(_)));

	if ctx.machine() {
		let items: Vec<Value> = shown.iter().map(|status| status_json(status)).collect();

		ctx.emit(json!({
			"checked": statuses.len(),
			"update_mode": engine.update_settings().await.mode.as_str(),
			"apps": items,
		}));
	} else if shown.is_empty() {
		ctx.note(if statuses.is_empty() {
			"Nothing is installed."
		} else {
			"Everything is up to date."
		});
	} else {
		let rows: Vec<Vec<String>> = shown
			.iter()
			.map(|status| {
				vec![
					status.name.clone(),
					status.installed_version.clone(),
					truncate(&status_line(status), 90),
				]
			})
			.collect();

		print_table(&["APP", "INSTALLED", "STATUS"], &rows);
	}

	// A lookup that failed is never reported as success.
	Ok(if failed {
		ExitStatus::Network
	} else {
		ExitStatus::Success
	})
}

pub(crate) async fn check(
	ctx: &Ctx,
	engine: &Arc<Engine>,
	args: &CheckArgs,
) -> Result<ExitStatus, CliError> {
	look(ctx, engine, args.app.as_deref(), false).await
}

pub(crate) async fn outdated(
	ctx: &Ctx,
	engine: &Arc<Engine>,
	args: &OutdatedArgs,
) -> Result<ExitStatus, CliError> {
	look(ctx, engine, None, !args.all).await
}

pub(crate) async fn history(
	ctx: &Ctx,
	engine: &Arc<Engine>,
	args: &HistoryArgs,
) -> Result<ExitStatus, CliError> {
	let mut history = engine
		.history(args.limit.saturating_mul(4).max(args.limit))
		.await?;

	if let Some(reference) = &args.app {
		let apps = engine.installed().await?;
		let wanted = find_app(&apps, reference)
			.map(|app| app.repository.clone())
			.ok();

		history.retain(|operation| wanted.is_some() && operation.app == wanted);
	}

	history.truncate(args.limit as usize);

	if ctx.machine() {
		ctx.emit(json!({
			"operations": history.iter().map(|operation| json!({
				"id": operation.id,
				"date": iso(operation.started_at),
				"operation": operation.kind,
				"result": operation.state,
				"source": operation.app,
				"detail": operation.detail,
			})).collect::<Vec<_>>(),
		}));

		return Ok(ExitStatus::Success);
	}

	if history.is_empty() {
		ctx.note("Nothing has been installed or removed yet.");

		return Ok(ExitStatus::Success);
	}

	let rows: Vec<Vec<String>> = history
		.iter()
		.map(|operation| {
			vec![
				when(operation.started_at),
				operation.kind.clone(),
				operation.state.clone(),
				operation.app.clone().unwrap_or_default(),
				truncate(operation.detail.as_deref().unwrap_or_default(), 50),
			]
		})
		.collect();

	print_table(&["WHEN", "OPERATION", "RESULT", "SOURCE", "DETAIL"], &rows);

	Ok(ExitStatus::Success)
}

pub(crate) async fn operations(
	ctx: &Ctx,
	engine: &Arc<Engine>,
	action: &OperationsAction,
) -> Result<ExitStatus, CliError> {
	match action {
		OperationsAction::List { limit } => {
			history(
				ctx,
				engine,
				&HistoryArgs {
					app: None,
					limit: *limit,
				},
			)
			.await
		}
		OperationsAction::Show { id } => {
			let all = engine.history(200).await?;
			let matches: Vec<_> = all
				.iter()
				.filter(|operation| operation.id.starts_with(id.as_str()))
				.collect();

			let [operation] = matches.as_slice() else {
				return Ok(refuse(
					ctx,
					if matches.is_empty() {
						"OPERATION_NOT_FOUND"
					} else {
						"OPERATION_AMBIGUOUS"
					},
					ExitStatus::Usage,
					if matches.is_empty() {
						"No operation has that ID."
					} else {
						"That ID matches more than one operation. Type more of it."
					},
				));
			};

			if ctx.machine() {
				ctx.emit(json!({
					"id": operation.id, "date": iso(operation.started_at), "operation": operation.kind,
					"result": operation.state, "source": operation.app, "detail": operation.detail,
				}));
			} else {
				println!("{}", operation.id);
				println!("  When:      {}", when(operation.started_at));
				println!("  Operation: {}", operation.kind);
				println!("  Result:    {}", operation.state);
				println!("  Source:    {}", operation.app.as_deref().unwrap_or("-"));
				println!(
					"  Detail:    {}",
					operation.detail.as_deref().unwrap_or("-")
				);
			}

			Ok(ExitStatus::Success)
		}
		OperationsAction::Cancel { .. } => Ok(refuse(
			ctx,
			"CANCEL_UNSUPPORTED",
			ExitStatus::Unsupported,
			"Operations cannot be cancelled yet. Each one runs to a safe point and then finishes or rolls back.",
		)),
		OperationsAction::Recover => {
			let now = time::OffsetDateTime::now_utc().unix_timestamp();
			let report = engine.lifecycle().recover(now).await?;

			if ctx.machine() {
				ctx.emit(json!({
					"rolled_back": report.rolled_back,
					"completed": report.completed,
					"needs_attention": report.needs_attention,
				}));
			} else {
				println!(
					"Rolled back {}, finished {}, and {} need attention.",
					report.rolled_back, report.completed, report.needs_attention
				);
			}

			Ok(if report.needs_attention > 0 {
				ExitStatus::RecoveryRequired
			} else {
				ExitStatus::Success
			})
		}
	}
}

fn verify_json(report: &launcher_core::VerifyReport) -> Value {
	json!({
		"app": report.name,
		"id": report.installation.map(|id| id.to_string()),
		"clean": report.is_clean(),
		"folder_is_the_launchers": report.ownership_ok,
		"files_checked": report.files_checked,
		"missing": report.missing,
		"modified": report.modified,
		"problems": report.problems,
	})
}

fn print_verify(report: &launcher_core::VerifyReport) {
	if report.is_clean() {
		println!("{}: ok ({} files match)", report.name, report.files_checked);

		return;
	}

	println!("{}: needs attention", report.name);

	for problem in &report.problems {
		println!("  {problem}");
	}

	for path in &report.modified {
		println!("  changed: {path}");
	}

	for path in &report.missing {
		println!("  missing: {path}");
	}
}

pub(crate) async fn verify(
	ctx: &Ctx,
	engine: &Arc<Engine>,
	args: &AppArgs,
) -> Result<ExitStatus, CliError> {
	let apps = engine.installed().await?;
	let app = find_app(&apps, &args.app)?;
	let report = engine.verify_installation(app.id).await?;

	if ctx.machine() {
		ctx.emit(verify_json(&report));
	} else {
		print_verify(&report);
	}

	Ok(if report.is_clean() {
		ExitStatus::Success
	} else {
		ExitStatus::Conflict
	})
}

pub(crate) async fn doctor(
	ctx: &Ctx,
	engine: &Arc<Engine>,
	args: &OptionalAppArgs,
) -> Result<ExitStatus, CliError> {
	if let Some(reference) = &args.app {
		return verify(
			ctx,
			engine,
			&AppArgs {
				app: reference.clone(),
			},
		)
		.await;
	}

	let report = engine.doctor().await?;

	if ctx.machine() {
		ctx.emit(json!({
			"healthy": report.is_healthy(),
			"database_ok": report.database_ok,
			"database": report.database_detail,
			"unfinished_operations": report.unfinished.iter().map(|operation| json!({
				"id": operation.id, "operation": operation.kind, "state": operation.state,
			})).collect::<Vec<_>>(),
			"catalog": report.catalog,
			"apps": report.apps.iter().map(verify_json).collect::<Vec<_>>(),
		}));
	} else {
		println!(
			"Database:   {}",
			if report.database_ok {
				"ok".to_owned()
			} else {
				report.database_detail.join("; ")
			}
		);
		println!("Catalog:    {}", report.catalog);

		if report.unfinished.is_empty() {
			println!("Operations: none unfinished");
		} else {
			println!(
				"Operations: {} unfinished. Run `remade operations recover`.",
				report.unfinished.len()
			);
		}

		for app in &report.apps {
			print_verify(app);
		}

		println!();
		println!(
			"{}",
			if report.is_healthy() {
				"Everything looks right."
			} else {
				"Something needs attention."
			}
		);
		println!("This check changes nothing.");
	}

	Ok(if !report.unfinished.is_empty() {
		ExitStatus::RecoveryRequired
	} else if report.is_healthy() {
		ExitStatus::Success
	} else {
		ExitStatus::Conflict
	})
}

pub(crate) async fn path(
	ctx: &Ctx,
	engine: &Arc<Engine>,
	args: &AppArgs,
) -> Result<ExitStatus, CliError> {
	let apps = engine.installed().await?;
	let app = find_app(&apps, &args.app)?;

	if ctx.machine() {
		ctx.emit(json!({ "app": app.name, "folder": app.root }));
	} else {
		println!("{}", app.root);
	}

	Ok(ExitStatus::Success)
}

pub(crate) async fn pins(ctx: &Ctx, engine: &Arc<Engine>) -> Result<ExitStatus, CliError> {
	let mut pinned = Vec::new();

	for app in engine.installed().await? {
		if engine
			.app_update_policy(app.id)
			.await?
			.pinned_release_id
			.is_some()
		{
			pinned.push(app);
		}
	}

	if ctx.machine() {
		ctx.emit(json!({ "pinned": pinned.iter().map(app_json).collect::<Vec<_>>() }));
	} else if pinned.is_empty() {
		ctx.note("No apps are pinned.");
	} else {
		let rows: Vec<Vec<String>> = pinned
			.iter()
			.map(|app| vec![app.name.clone(), app.version.clone()])
			.collect();

		print_table(&["APP", "PINNED AT"], &rows);
	}

	Ok(ExitStatus::Success)
}
