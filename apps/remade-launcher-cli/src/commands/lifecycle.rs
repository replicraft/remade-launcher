//! Planning and applying: plan, apply, install, update, uninstall, launch,
//! forget, pin, and unpin.
//!
//! Every change goes through the same two steps the desktop app uses: a sealed
//! plan, then an approval of exactly that plan.

use std::io::IsTerminal;
use std::sync::Arc;

use launcher_core::{
	Applied, Approval, CustomPreparation, Engine, InstalledApp, OperatorUpdate, Preparation, Ready,
	RepairOutcome, SavedPlan, UpdateState, custom_plan,
};
use launcher_model::InstallationId;
use launcher_resolver::{EvidenceLevel, Fit};
use serde_json::{Value, json};

use super::installed::{print_verify, verify_json};
use super::shared::{Found, app_json, ensure_catalog, find_any, find_app};
use crate::cli::{
	AppArgs, ApplyArgs, DownloadArgs, ForgetArgs, InstallArgs, PlanAction, PlanInstallArgs,
	RepairArgs, UninstallArgs, UpdateArgs,
};
use crate::context::Ctx;
use crate::errors::CliError;
use crate::exit::ExitStatus;
use crate::output::{bytes, iso, refuse, when};
use crate::progress::{DownloadProgress, Printer};
use crate::prompt;

// ---------------------------------------------------------------------------
// Making a plan
// ---------------------------------------------------------------------------

fn evidence(level: EvidenceLevel) -> &'static str {
	match level {
		EvidenceLevel::HostDigest => {
			"the checksum GitHub reports (shows the file is intact, not who made it)"
		}
		EvidenceLevel::PinnedSha256 => "a checksum pinned in the signed catalog",
		EvidenceLevel::PublisherSignature => "the publisher's signature",
		EvidenceLevel::LocalObserved => {
			"nothing beyond the download itself (no checksum was published)"
		}
	}
}

fn evidence_code(level: EvidenceLevel) -> &'static str {
	match level {
		EvidenceLevel::HostDigest => "host_digest",
		EvidenceLevel::PinnedSha256 => "pinned_sha256",
		EvidenceLevel::PublisherSignature => "publisher_signature",
		EvidenceLevel::LocalObserved => "local_observed",
	}
}

fn plan_json(id: &str, ready: &Ready) -> Value {
	let body = ready.plan.body();

	json!({
		"plan_id": id,
		"plan_digest": ready.plan.digest_hex(),
		"operation": body.operation.verb(),
		"app": ready.meta.display_name,
		"origin": if ready.unreviewed { "custom" } else { "curated" },
		"reviewed_by_remade": !ready.unreviewed,
		"source": ready.source_display,
		"version": body.release.tag,
		"file": { "name": body.asset.name, "size": body.asset.size },
		"checked_against": evidence_code(body.install.evidence),
		"expires_at": iso(body.expires_at),
		"explanation": ready.candidate.explanation,
		"notes": ready.notes,
		"rebuilt_artifact": ready.rebuilt,
		"approval": {
			"confirmation_required": true,
			"accept_unreviewed_source": ready.unreviewed.then_some(&ready.source_display),
		},
	})
}

fn print_plan(id: &str, ready: &Ready) {
	let body = ready.plan.body();

	println!(
		"Plan {id}: {} {}",
		body.operation.verb(),
		ready.meta.display_name
	);
	println!("  Source:          {}", ready.source_display);

	if ready.unreviewed {
		println!(
			"  Warning:         Remade has not reviewed this source. Only install software from people you trust."
		);
	} else {
		println!("  Reviewed:        listed in the verified Remade catalog");
	}

	println!("  Version:         {}", body.release.tag);
	println!(
		"  File:            {} ({})",
		body.asset.name,
		bytes(body.asset.size)
	);
	println!("  Checked against: {}", evidence(body.install.evidence));
	println!("  Valid until:     {}", when(body.expires_at));

	for line in &ready.candidate.explanation {
		println!("  - {line}");
	}

	for note in &ready.notes {
		println!("  Note: {note}");
	}

	for skipped in &ready.candidate.skipped_newer {
		println!(
			"  Newer release {} skipped: {}",
			skipped.tag, skipped.reason
		);
	}
}

/// What to do with a lookup of a user-added repository.
enum Custom {
	Ready(Box<Ready>),
	/// The file could not be chosen, with the explanation to show.
	Choose(String),
}

fn pick_file(lookup: &CustomPreparation, wanted: Option<&str>) -> Option<u64> {
	if let Some(name) = wanted {
		return lookup
			.options
			.iter()
			.find(|option| option.asset.name == name)
			.map(|option| option.asset.id);
	}

	let mut fitting = lookup
		.options
		.iter()
		.filter(|option| option.fit == Fit::Matches && option.managed);
	let first = fitting.next()?;

	fitting.next().is_none().then_some(first.asset.id)
}

fn describe_options(lookup: &CustomPreparation, wanted: Option<&str>) -> String {
	let mut text = match wanted {
		Some(name) => format!("This release has no file named \"{name}\"."),
		None => {
			"The launcher cannot tell which file to install. Choose one with --file:".to_owned()
		}
	};

	for option in &lookup.options {
		let note = match (option.managed, option.fit) {
			(false, _) => "download only",
			(true, Fit::Matches) => "named for this device",
			(true, Fit::Unknown) => "name does not say which device",
			(true, Fit::Mismatch) => "named for another device",
		};

		text.push_str(&format!(
			"\n  {} ({}, {note})",
			option.asset.name,
			bytes(option.asset.size)
		));
	}

	text
}

async fn custom_ready(
	engine: &Arc<Engine>,
	url: &str,
	file: Option<&str>,
) -> Result<Custom, CliError> {
	let lookup = engine.prepare_custom(url).await?;

	let Some(asset_id) = pick_file(&lookup, file) else {
		return Ok(Custom::Choose(describe_options(&lookup, file)));
	};

	let now = time::OffsetDateTime::now_utc().unix_timestamp();

	Ok(Custom::Ready(Box::new(custom_plan(
		&lookup, asset_id, now,
	)?)))
}

/// The plan for `target`: a catalog name, or a GitHub link.
async fn ready_for_target(
	ctx: &Ctx,
	engine: &Arc<Engine>,
	target: &str,
	file: Option<&str>,
) -> Result<Result<Ready, ExitStatus>, CliError> {
	if ctx.offline {
		return Ok(Err(refuse(
			ctx,
			"OFFLINE",
			ExitStatus::Network,
			"Planning an install needs the network.",
		)));
	}

	if target.starts_with("https://") || target.starts_with("http://") {
		return Ok(match custom_ready(engine, target, file).await? {
			Custom::Ready(ready) => Ok(*ready),
			Custom::Choose(text) => Err(refuse(ctx, "ASSET_AMBIGUOUS", ExitStatus::Usage, &text)),
		});
	}

	ensure_catalog(ctx, engine).await?;

	Ok(match engine.prepare_install(target).await? {
		Preparation::Ready(ready) => Ok(*ready),
		Preparation::Unavailable(why) => Err(refuse(
			ctx,
			"NO_COMPATIBLE_PACKAGE",
			ExitStatus::Unsupported,
			&why,
		)),
		Preparation::Choose(candidates) => {
			let names: Vec<String> = candidates
				.iter()
				.map(|candidate| candidate.asset.name.clone())
				.collect();

			Err(refuse(
				ctx,
				"ASSET_AMBIGUOUS",
				ExitStatus::Unsupported,
				&format!(
					"Several packages fit this device equally well, and the launcher will not guess: {}. This is a gap in the catalog entry.",
					names.join(", ")
				),
			))
		}
	})
}

// ---------------------------------------------------------------------------
// Approval
// ---------------------------------------------------------------------------

/// Builds the approval for `ready`.
///
/// A source Remade has not reviewed needs the person to name that exact
/// repository: on the command line with `--accept-unreviewed-source`, or by
/// typing it at the prompt. `--yes` alone is never enough for one.
fn approval_for(
	ctx: &Ctx,
	ready: &Ready,
	yes: bool,
	accepted: Option<&str>,
	question: &str,
) -> Result<Approval, CliError> {
	if !ready.unreviewed {
		let confirmed = prompt::confirm(ctx, question, yes)?;

		return Ok(Approval {
			actor: "cli".into(),
			confirmed,
			accepted_unreviewed_source: None,
		});
	}

	// Naming the repository is itself the confirmation when it is typed.
	if let Some(accepted) = accepted {
		let confirmed = prompt::confirm(ctx, question, yes)?;

		return Ok(Approval {
			actor: "cli".into(),
			confirmed,
			accepted_unreviewed_source: Some(accepted.to_owned()),
		});
	}

	if ctx.no_input || !std::io::stdin().is_terminal() {
		return Err(CliError::new(
			"UNVERIFIED_SOURCE_APPROVAL_REQUIRED",
			ExitStatus::ApprovalRequired,
			format!(
				"Remade has not reviewed {0}. To install from it, pass --accept-unreviewed-source {0}.",
				ready.source_display
			),
		));
	}

	eprint!(
		"Remade has not reviewed this source. Type {} to accept it: ",
		ready.source_display
	);

	let mut typed = String::new();
	let _ = std::io::stdin().read_line(&mut typed);

	if typed.trim() != ready.source_display {
		return Err(CliError::approval(
			"That is not the repository name, so nothing was installed.",
		));
	}

	Ok(Approval {
		actor: "cli".into(),
		confirmed: true,
		accepted_unreviewed_source: Some(typed.trim().to_owned()),
	})
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

pub(crate) async fn plan(
	ctx: &Ctx,
	engine: &Arc<Engine>,
	action: &PlanAction,
) -> Result<ExitStatus, CliError> {
	match action {
		PlanAction::Install(PlanInstallArgs { target, file }) => {
			let ready = match ready_for_target(ctx, engine, target, file.as_deref()).await? {
				Ok(ready) => ready,
				Err(status) => return Ok(status),
			};
			let id = engine.save_ready_plan(&ready, None).await?;

			show_plan(ctx, &id, &ready, None);

			Ok(ExitStatus::Success)
		}
		PlanAction::Update { app } => {
			let apps = engine.installed().await?;
			let app = find_app(&apps, app)?;

			match prepare_update(ctx, engine, app).await? {
				Ok(ready) => {
					let id = engine.save_ready_plan(&ready, Some(app.id)).await?;

					show_plan(ctx, &id, &ready, Some(app));

					Ok(ExitStatus::Success)
				}
				Err(status) => Ok(status),
			}
		}
		PlanAction::Uninstall { app } => {
			let apps = engine.installed().await?;
			let app = find_app(&apps, app)?;
			let plan = engine.plan_uninstall(app.id).await?;

			if ctx.machine() {
				ctx.emit(json!({
					"plan_id": plan.id, "operation": "uninstall", "app": plan.name, "source": plan.repository,
					"version": plan.version, "owned_files": plan.owned_files, "expires_at": iso(plan.expires_at),
					"keeps": "files you edited, and files the app made itself",
					"approval": { "confirmation_required": true },
				}));
			} else {
				println!("Plan {}: uninstall {} {}", plan.id, plan.name, plan.version);
				println!(
					"  Removes up to {} files the launcher installed.",
					plan.owned_files
				);
				println!("  Keeps files you edited and files the app saved itself.");
				println!("  Valid until: {}", when(plan.expires_at));
				println!("Run `remade apply {}` to do it.", plan.id);
			}

			Ok(ExitStatus::Success)
		}
		PlanAction::Repair { app } => {
			let apps = engine.installed().await?;
			let app = find_app(&apps, app)?;

			match diagnose(ctx, engine, app).await? {
				Ok(Some(ready)) => {
					let id = engine.save_ready_plan(&ready, Some(app.id)).await?;

					show_repair_plan(ctx, &id, &ready, app);

					Ok(ExitStatus::Success)
				}
				Ok(None) => Ok(ExitStatus::Success),
				Err(status) => Ok(status),
			}
		}
		PlanAction::Show { id } => {
			match engine.load_plan(id).await? {
				SavedPlan::Install {
					ready,
					update_of,
					id,
				} => {
					let apps = engine.installed().await?;
					let app = update_of.and_then(|target| apps.iter().find(|app| app.id == target));

					show_plan(ctx, &id, &ready, app);
				}
				SavedPlan::Uninstall(plan) => {
					if ctx.machine() {
						ctx.emit(json!({
							"plan_id": plan.id, "operation": "uninstall", "app": plan.name,
							"version": plan.version, "owned_files": plan.owned_files, "expires_at": iso(plan.expires_at),
						}));
					} else {
						println!(
							"Plan {}: uninstall {} {} ({} files)",
							plan.id, plan.name, plan.version, plan.owned_files
						);
					}
				}
			}

			Ok(ExitStatus::Success)
		}
	}
}

fn show_plan(ctx: &Ctx, id: &str, ready: &Ready, app: Option<&InstalledApp>) {
	if ctx.machine() {
		let mut value = plan_json(id, ready);

		if let (Some(app), Some(object)) = (app, value.as_object_mut()) {
			object.insert(
				"replaces".into(),
				json!({ "id": app.id.to_string(), "version": app.version }),
			);
		}

		ctx.emit(value);

		return;
	}

	print_plan(id, ready);

	if let Some(app) = app {
		println!("  Replaces:        {} {}", app.name, app.version);
	}

	println!("Run `remade apply {id}` to approve it.");
}

/// Looks for an update for `app` and returns its plan, or the status a
/// refusal already reported.
async fn prepare_update(
	ctx: &Ctx,
	engine: &Arc<Engine>,
	app: &InstalledApp,
) -> Result<Result<Ready, ExitStatus>, CliError> {
	if ctx.offline {
		return Ok(Err(refuse(
			ctx,
			"OFFLINE",
			ExitStatus::Network,
			"Updating needs the network.",
		)));
	}

	ensure_catalog(ctx, engine).await?;

	if engine
		.app_update_policy(app.id)
		.await?
		.pinned_release_id
		.is_some()
	{
		return Ok(Err(refuse(
			ctx,
			"APP_PINNED",
			ExitStatus::Conflict,
			&format!(
				"{} is pinned to {}. Run `remade unpin` first.",
				app.name, app.version
			),
		)));
	}

	Ok(match engine.prepare_update(app.id).await? {
		Preparation::Ready(ready) => Ok(*ready),
		Preparation::Unavailable(why) => Err(refuse(
			ctx,
			"NO_COMPATIBLE_UPDATE",
			ExitStatus::Unsupported,
			&why,
		)),
		Preparation::Choose(_) => Err(refuse(
			ctx,
			"ASSET_AMBIGUOUS",
			ExitStatus::Unsupported,
			"Several packages fit, so no update was chosen.",
		)),
	})
}

/// Looks at one app. Prints what is wrong, and returns the plan to put it
/// back if there is one. `Ok(None)` means nothing needed doing.
async fn diagnose(
	ctx: &Ctx,
	engine: &Arc<Engine>,
	app: &InstalledApp,
) -> Result<Result<Option<Ready>, ExitStatus>, CliError> {
	// Looking is free, so it works offline. Only a plan needs the network.
	let report = engine.verify_installation(app.id).await?;

	if !report.is_clean() && ctx.offline {
		if !ctx.machine() {
			print_verify(&report);
		}

		return Ok(Err(refuse(
			ctx,
			"OFFLINE",
			ExitStatus::Network,
			"Repairing needs the network to fetch the app again.",
		)));
	}

	if !report.is_clean() {
		ensure_catalog(ctx, engine).await?;
	}

	let assessment = engine.assess_repair(app.id).await?;

	match assessment.outcome {
		RepairOutcome::NotNeeded => {
			if ctx.machine() {
				ctx.emit(
					json!({ "repair": "not_needed", "diagnosis": verify_json(&assessment.report) }),
				);
			} else {
				println!(
					"{}: nothing to repair ({} files match).",
					app.name, assessment.report.files_checked
				);
			}

			Ok(Ok(None))
		}
		RepairOutcome::Unavailable(why) => {
			if !ctx.machine() {
				print_verify(&assessment.report);
			}

			Ok(Err(refuse(
				ctx,
				"REPAIR_UNAVAILABLE",
				ExitStatus::Unsupported,
				&why,
			)))
		}
		RepairOutcome::Ready(ready) => {
			if !ctx.machine() {
				print_verify(&assessment.report);
			}

			Ok(Ok(Some(*ready)))
		}
	}
}

fn show_repair_plan(ctx: &Ctx, id: &str, ready: &Ready, app: &InstalledApp) {
	if ctx.machine() {
		let mut value = plan_json(id, ready);

		if let Some(object) = value.as_object_mut() {
			object.insert("operation".into(), json!("repair"));
			object.insert(
				"replaces".into(),
				json!({ "id": app.id.to_string(), "version": app.version }),
			);
		}

		ctx.emit(value);

		return;
	}

	print_plan(id, ready);
	println!(
		"  Replaces:        {} {} (the damaged copy)",
		app.name, app.version
	);
	println!("Run `remade apply {id}` to approve it.");
}

pub(crate) async fn repair(
	ctx: &Arc<Ctx>,
	engine: &Arc<Engine>,
	args: &RepairArgs,
) -> Result<ExitStatus, CliError> {
	let apps = engine.installed().await?;
	let app = find_app(&apps, &args.app)?;

	let ready = match diagnose(ctx, engine, app).await? {
		Ok(Some(ready)) => ready,
		Ok(None) => return Ok(ExitStatus::Success),
		Err(status) => return Ok(status),
	};

	let id = engine.save_ready_plan(&ready, Some(app.id)).await?;

	if args.preview {
		show_repair_plan(ctx, &id, &ready, app);

		return Ok(ExitStatus::Success);
	}

	print_review(ctx, &ready);

	let approval = approval_for(
		ctx,
		&ready,
		args.yes,
		None,
		&format!(
			"Put {} back from {}?",
			app.name, ready.candidate.release.tag
		),
	)?;

	if !approval.confirmed {
		return Ok(refuse(
			ctx,
			"APPROVAL_REQUIRED",
			ExitStatus::ApprovalRequired,
			"Cancelled. Nothing was changed.",
		));
	}

	let applied = run_plan(ctx, engine, &id, &approval).await?;
	let after = engine.installed().await?;

	report_applied(ctx, &applied, &after, &id);

	Ok(ExitStatus::Success)
}

/// Downloads the file a plan names, checks it, and saves it. Nothing is
/// unpacked, recorded, or run.
pub(crate) async fn download(
	ctx: &Arc<Ctx>,
	engine: &Arc<Engine>,
	args: &DownloadArgs,
) -> Result<ExitStatus, CliError> {
	let ready = match ready_for_target(ctx, engine, &args.target, args.file.as_deref()).await? {
		Ok(ready) => ready,
		Err(status) => return Ok(status),
	};

	print_review(ctx, &ready);

	let download = DownloadProgress::start(ctx);
	let mut progress = download.callback();

	let saved = engine
		.download_artifact(&ready, std::path::Path::new(&args.out), &mut progress)
		.await;

	download.finish();

	let saved = saved?;

	if ctx.machine() {
		ctx.emit(json!({
			"saved_to": saved.path.display().to_string(),
			"size": saved.size,
			"sha256": saved.sha256,
			"checked_against_expected_sha256": saved.matched_expected_sha256,
			"source": ready.source_display,
			"version": ready.plan.body().release.tag,
			"reviewed_by_remade": !ready.unreviewed,
			"installed": false,
			"executed": false,
		}));
	} else {
		println!("Saved {} ({}).", saved.path.display(), bytes(saved.size));
		println!("SHA-256 {}", saved.sha256);
		println!("It was checked, not installed or run. Use `remade install` to install it.");
	}

	Ok(ExitStatus::Success)
}

/// Turns Ctrl-C into a request to stop at the next safe point. Once files
/// are moving into place the operation finishes instead, so it never leaves
/// a half-installed app behind.
fn stop_on_interrupt(engine: &Arc<Engine>) -> tokio::task::JoinHandle<()> {
	let engine = Arc::clone(engine);

	tokio::spawn(async move {
		while tokio::signal::ctrl_c().await.is_ok() {
			if !engine.lifecycle().request_cancel() {
				eprintln!("Too late to stop safely. Finishing the install.");
			}
		}
	})
}

/// Runs a saved plan with progress, and reports the outcome.
async fn run_plan(
	ctx: &Arc<Ctx>,
	engine: &Arc<Engine>,
	id: &str,
	approval: &Approval,
) -> Result<Applied, CliError> {
	let printer = Printer::start(engine, ctx);
	let interrupt = stop_on_interrupt(engine);
	let result = engine.apply_plan(id, approval).await;

	interrupt.abort();
	printer.finish().await;

	Ok(result?)
}

fn report_applied(ctx: &Ctx, applied: &Applied, apps_after: &[InstalledApp], plan_id: &str) {
	match applied {
		Applied::Installed(id) => {
			let app = apps_after.iter().find(|app| app.id == *id);

			if ctx.machine() {
				ctx.emit(
					json!({ "plan_id": plan_id, "result": "completed", "installation": app.map(app_json) }),
				);
				ctx.event(json!({ "phase": "completed", "installation_id": id.to_string() }));
			} else if let Some(app) = app {
				println!("Installed {} {} in {}", app.name, app.version, app.root);

				if app.launchable {
					println!(
						"Start it with: remade launch {}",
						app.catalog_slug.as_deref().unwrap_or(&app.name)
					);
				}
			}
		}
		Applied::Removed(report) => {
			if ctx.machine() {
				ctx.emit(json!({
					"plan_id": plan_id, "result": "completed", "removed_files": report.removed_files,
					"kept_edited": report.kept_modified, "kept_unknown": report.kept_unknown,
					"leftover_folder": report.leftover_folder,
				}));
				ctx.event(json!({ "phase": "completed" }));
			} else {
				println!("Uninstalled. {} files removed.", report.removed_files);

				if !report.is_clean() {
					eprintln!("Kept so nothing you made is lost:");

					for file in report
						.kept_modified
						.iter()
						.chain(&report.kept_unknown)
						.take(10)
					{
						eprintln!("  {file}");
					}

					if let Some(folder) = &report.leftover_folder {
						eprintln!("in {folder}");
					}
				}
			}
		}
	}
}

pub(crate) async fn apply(
	ctx: &Arc<Ctx>,
	engine: &Arc<Engine>,
	args: &ApplyArgs,
) -> Result<ExitStatus, CliError> {
	let saved = engine.load_plan(&args.plan).await?;

	let (approval, id) = match &saved {
		SavedPlan::Install {
			ready,
			id,
			update_of,
		} => {
			let verb = if update_of.is_some() {
				"Update"
			} else {
				"Install"
			};

			print_review(ctx, ready);

			(
				approval_for(
					ctx,
					ready,
					args.yes,
					args.accept_unreviewed_source.as_deref(),
					&format!("{verb} {}?", ready.meta.display_name),
				)?,
				id.clone(),
			)
		}
		SavedPlan::Uninstall(plan) => {
			ctx.note(format!(
				"This removes the files the launcher installed for {} {}.",
				plan.name, plan.version
			));
			ctx.note("Files the app saved in its folder, and files you edited, are kept.");

			let confirmed = prompt::confirm(ctx, &format!("Uninstall {}?", plan.name), args.yes)?;

			(
				Approval {
					actor: "cli".into(),
					confirmed,
					accepted_unreviewed_source: None,
				},
				plan.id.clone(),
			)
		}
	};

	if !approval.confirmed {
		return Ok(refuse(
			ctx,
			"APPROVAL_REQUIRED",
			ExitStatus::ApprovalRequired,
			"Cancelled. Nothing was changed.",
		));
	}

	let applied = run_plan(ctx, engine, &id, &approval).await?;
	let apps = engine.installed().await?;

	report_applied(ctx, &applied, &apps, &id);

	Ok(ExitStatus::Success)
}

fn print_review(ctx: &Ctx, ready: &Ready) {
	if ctx.machine() || ctx.quiet {
		return;
	}

	let body = ready.plan.body();

	eprintln!("{} {}", body.operation.verb(), ready.meta.display_name);
	eprintln!("  Source:          {}", ready.source_display);

	if ready.unreviewed {
		eprintln!(
			"  Warning:         Remade has not reviewed this source. Only install software from people you trust."
		);
	} else {
		eprintln!("  Reviewed:        listed in the verified Remade catalog");
	}

	eprintln!("  Version:         {}", body.release.tag);
	eprintln!(
		"  File:            {} ({})",
		body.asset.name,
		bytes(body.asset.size)
	);
	eprintln!("  Checked against: {}", evidence(body.install.evidence));

	for note in &ready.notes {
		eprintln!("  Note: {note}");
	}
}

pub(crate) async fn install(
	ctx: &Arc<Ctx>,
	engine: &Arc<Engine>,
	args: &InstallArgs,
) -> Result<ExitStatus, CliError> {
	let ready = match ready_for_target(ctx, engine, &args.target, args.file.as_deref()).await? {
		Ok(ready) => ready,
		Err(status) => return Ok(status),
	};

	let id = engine.save_ready_plan(&ready, None).await?;

	print_review(ctx, &ready);

	let approval = approval_for(
		ctx,
		&ready,
		args.yes,
		args.accept_unreviewed_source.as_deref(),
		&format!(
			"Install {}{}?",
			ready.meta.display_name,
			if ready.unreviewed {
				" from a source Remade has not reviewed"
			} else {
				""
			}
		),
	)?;

	if !approval.confirmed {
		return Ok(refuse(
			ctx,
			"APPROVAL_REQUIRED",
			ExitStatus::ApprovalRequired,
			"Cancelled. Nothing was changed.",
		));
	}

	let applied = run_plan(ctx, engine, &id, &approval).await?;
	let apps = engine.installed().await?;

	report_applied(ctx, &applied, &apps, &id);

	Ok(ExitStatus::Success)
}

pub(crate) async fn update(
	ctx: &Arc<Ctx>,
	engine: &Arc<Engine>,
	args: &UpdateArgs,
) -> Result<ExitStatus, CliError> {
	if args.all || args.app.is_none() {
		return update_all(ctx, engine, args.preview, args.yes).await;
	}

	let apps = engine.installed().await?;
	let app = find_app(&apps, args.app.as_deref().unwrap_or_default())?;

	let ready = match prepare_update(ctx, engine, app).await? {
		Ok(ready) => ready,
		Err(status) => return Ok(status),
	};

	let id = engine.save_ready_plan(&ready, Some(app.id)).await?;

	if args.preview {
		show_plan(ctx, &id, &ready, Some(app));

		return Ok(ExitStatus::Success);
	}

	print_review(ctx, &ready);

	let approval = approval_for(
		ctx,
		&ready,
		args.yes,
		None,
		&format!("Update {} to {}?", app.name, ready.candidate.release.tag),
	)?;

	if !approval.confirmed {
		return Ok(refuse(
			ctx,
			"APPROVAL_REQUIRED",
			ExitStatus::ApprovalRequired,
			"Cancelled. Nothing was changed.",
		));
	}

	let applied = run_plan(ctx, engine, &id, &approval).await?;
	let after = engine.installed().await?;

	report_applied(ctx, &applied, &after, &id);

	Ok(ExitStatus::Success)
}

/// Every app that has an update: a preview of all of them together, then one
/// approval, then each applied on its own so one failure stops no other.
pub(crate) async fn update_all(
	ctx: &Arc<Ctx>,
	engine: &Arc<Engine>,
	preview: bool,
	yes: bool,
) -> Result<ExitStatus, CliError> {
	if ctx.offline {
		return Ok(refuse(
			ctx,
			"OFFLINE",
			ExitStatus::Network,
			"Updating needs the network.",
		));
	}

	ensure_catalog(ctx, engine).await?;

	let plans: Vec<OperatorUpdate> = engine.plan_operator_updates(None).await?;
	let (ready, others): (Vec<_>, Vec<_>) =
		plans.into_iter().partition(|item| item.ready.is_some());

	let mut status = ExitStatus::Success;

	for item in &others {
		if matches!(item.status.state, UpdateState::CheckFailed(_)) {
			status = status.worst(ExitStatus::Network);
		}
	}

	if ctx.machine() && preview {
		ctx.emit(json!({
			"will_update": ready.iter().map(|item| json!({
				"app": item.status.name,
				"from": item.status.installed_version,
				"to": item.ready.as_ref().map(|ready| ready.candidate.release.tag.clone()),
			})).collect::<Vec<_>>(),
			"not_updating": others.iter().map(|item| super::shared::status_json(&item.status)).collect::<Vec<_>>(),
		}));

		return Ok(status);
	}

	if !ctx.machine() {
		for item in &ready {
			let target = item
				.ready
				.as_ref()
				.map_or("?", |ready| ready.candidate.release.tag.as_str());

			println!(
				"{}: {} -> {target}",
				item.status.name, item.status.installed_version
			);
		}

		for item in &others {
			println!(
				"{}: {}",
				item.status.name,
				super::shared::status_line(&item.status)
			);
		}
	}

	if ready.is_empty() || preview {
		if ready.is_empty() {
			ctx.note("Nothing to update.");
		}

		if ctx.machine() {
			ctx.emit(json!({ "updated": [], "failed": [] }));
		}

		return Ok(status);
	}

	if !prompt::confirm(ctx, &format!("Update {} app(s)?", ready.len()), yes)? {
		return Ok(refuse(
			ctx,
			"APPROVAL_REQUIRED",
			ExitStatus::ApprovalRequired,
			"Cancelled. Nothing was changed.",
		));
	}

	let mut updated = Vec::new();
	let mut failed = Vec::new();

	for item in ready {
		let Some(plan) = item.ready else { continue };
		let installation: InstallationId = item.status.installation;

		let id = engine.save_ready_plan(&plan, Some(installation)).await?;
		let approval = Approval {
			actor: "cli".into(),
			confirmed: true,
			accepted_unreviewed_source: None,
		};

		match run_plan(ctx, engine, &id, &approval).await {
			Ok(_) => {
				ctx.note(format!(
					"Updated {} to {}.",
					item.status.name, plan.candidate.release.tag
				));
				updated.push(
					json!({ "app": item.status.name, "version": plan.candidate.release.tag }),
				);
			}
			Err(error) => {
				status = status.worst(error.status);
				ctx.warn(format!(
					"{}: update failed. {}",
					item.status.name, error.message
				));
				failed.push(
					json!({ "app": item.status.name, "code": error.code, "message": error.message }),
				);
			}
		}
	}

	if ctx.machine() {
		ctx.emit(json!({ "updated": updated, "failed": failed }));
	}

	// A partly failed batch is never a success.
	Ok(if failed.is_empty() {
		status
	} else {
		status.worst(ExitStatus::Incomplete)
	})
}

pub(crate) async fn uninstall(
	ctx: &Arc<Ctx>,
	engine: &Arc<Engine>,
	args: &UninstallArgs,
) -> Result<ExitStatus, CliError> {
	let apps = engine.installed().await?;
	let app = match find_any(engine, &apps, &args.app).await? {
		Found::Managed(app) => app,
		Found::Watched(watched) => return Ok(super::adopt::not_managed(ctx, &watched, "remove")),
	};
	let plan = engine.plan_uninstall(app.id).await?;

	ctx.note(format!(
		"This removes the files the launcher installed for {} {}.",
		app.name, app.version
	));
	ctx.note("Files the app saved in its folder, and files you edited, are kept.");

	let confirmed = prompt::confirm(ctx, &format!("Uninstall {}?", app.name), args.yes)?;

	if !confirmed {
		return Ok(refuse(
			ctx,
			"APPROVAL_REQUIRED",
			ExitStatus::ApprovalRequired,
			"Cancelled. Nothing was changed.",
		));
	}

	let approval = Approval {
		actor: "cli".into(),
		confirmed,
		accepted_unreviewed_source: None,
	};
	let applied = run_plan(ctx, engine, &plan.id, &approval).await?;

	report_applied(ctx, &applied, &[], &plan.id);

	Ok(ExitStatus::Success)
}

pub(crate) async fn launch(
	ctx: &Ctx,
	engine: &Arc<Engine>,
	args: &AppArgs,
) -> Result<ExitStatus, CliError> {
	let apps = engine.installed().await?;
	let app = match find_any(engine, &apps, &args.app).await? {
		Found::Managed(app) => app,
		Found::Watched(watched) => return super::adopt::launch(ctx, engine, &watched).await,
	};

	engine.lifecycle().launch(app.id).await?;

	if ctx.machine() {
		ctx.emit(json!({ "app": app.name, "started": true }));
	} else {
		ctx.note(format!("Started {}.", app.name));
	}

	Ok(ExitStatus::Success)
}

pub(crate) async fn forget(
	ctx: &Ctx,
	engine: &Arc<Engine>,
	args: &ForgetArgs,
) -> Result<ExitStatus, CliError> {
	let apps = engine.installed().await?;
	let app = match find_any(engine, &apps, &args.app).await? {
		Found::Managed(app) => app,
		Found::Watched(watched) => {
			return super::adopt::forget(ctx, engine, &watched, args.yes).await;
		}
	};

	ctx.note(format!(
		"The launcher will stop managing {}. Its files stay where they are.",
		app.name
	));

	if !prompt::confirm(ctx, &format!("Forget {}?", app.name), args.yes)? {
		return Ok(refuse(
			ctx,
			"APPROVAL_REQUIRED",
			ExitStatus::ApprovalRequired,
			"Cancelled. Nothing was changed.",
		));
	}

	engine.forget(app.id).await?;

	if ctx.machine() {
		ctx.emit(json!({ "app": app.name, "forgotten": true, "files_left_in_place": app.root }));
	} else {
		println!("Forgot {}. Its files are still in {}.", app.name, app.root);
	}

	Ok(ExitStatus::Success)
}

pub(crate) async fn pin(
	ctx: &Ctx,
	engine: &Arc<Engine>,
	args: &AppArgs,
	pinned: bool,
) -> Result<ExitStatus, CliError> {
	let apps = engine.installed().await?;
	let app = find_app(&apps, &args.app)?;

	if pinned {
		engine.pin(app.id).await?;
	} else {
		engine.unpin(app.id).await?;
	}

	if ctx.machine() {
		ctx.emit(json!({ "app": app.name, "pinned": pinned, "version": app.version }));
	} else if pinned {
		println!(
			"{} is pinned at {}. It will not update until you unpin it.",
			app.name, app.version
		);
	} else {
		println!("{} can update again.", app.name);
	}

	Ok(ExitStatus::Success)
}
