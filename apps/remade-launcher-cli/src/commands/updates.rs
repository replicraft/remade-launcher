//! `remade updates`: the global mode, the minimum release age, per-app policy,
//! and update checks.
//!
//! Changing any of these only changes a setting. Nothing here starts an
//! update, and none of it can authorize one that verification would refuse.

use std::sync::Arc;

use launcher_core::{Engine, ModeConsent};
use launcher_model::{MinimumAge, UpdateMode};
use launcher_storage::AppPolicy;
use serde_json::json;

use super::lifecycle;
use super::shared::find_app;
use crate::cli::{
	AgeAction, DecisionsArgs, ModeAction, ModeSetArgs, PolicyArgs, RunArgs, UpdatesAction,
};
use crate::context::Ctx;
use crate::errors::CliError;
use crate::exit::ExitStatus;
use crate::output::{iso, print_table, when};

pub(crate) async fn run(
	ctx: &Arc<Ctx>,
	engine: &Arc<Engine>,
	action: &UpdatesAction,
) -> Result<ExitStatus, CliError> {
	match action {
		UpdatesAction::Mode { action: None } => mode_show(ctx, engine).await,
		UpdatesAction::Mode {
			action: Some(ModeAction::Set(args)),
		} => mode_set(ctx, engine, args).await,
		UpdatesAction::Age { action: None } => age_show(ctx, engine).await,
		UpdatesAction::Age {
			action: Some(AgeAction::Set { duration, app }),
		} => age_set(ctx, engine, duration, app.as_deref()).await,
		UpdatesAction::Age {
			action: Some(AgeAction::Reset { app }),
		} => age_reset(ctx, engine, app).await,
		UpdatesAction::Policy(args) => policy(ctx, engine, args).await,
		UpdatesAction::Check => {
			super::installed::check(ctx, engine, &crate::cli::CheckArgs { app: None }).await
		}
		UpdatesAction::Run(RunArgs { dry_run, yes }) => {
			lifecycle::update_all(ctx, engine, *dry_run, *yes).await
		}
		UpdatesAction::Decisions(args) => decisions(ctx, engine, args).await,
	}
}

async fn mode_show(ctx: &Ctx, engine: &Arc<Engine>) -> Result<ExitStatus, CliError> {
	let settings = engine.update_settings().await;
	let background = crate::background::describe(engine).await;

	if ctx.machine() {
		ctx.emit(json!({
			"schema_version": 1,
			"update_mode": settings.mode.as_str(),
			"minimum_release_age_seconds": settings.minimum_age.seconds(),
			"daemon": background,
		}));
	} else {
		println!("{}", settings.mode.label());
		println!("{}", settings.mode.description());
		println!();
		println!("Minimum release age: {}", settings.minimum_age.short());
		println!(
			"Background updater:  {}",
			background["actual_state"].as_str().unwrap_or("unknown")
		);
	}

	Ok(ExitStatus::Success)
}

pub(crate) async fn mode_set_public(
	ctx: &Ctx,
	engine: &Arc<Engine>,
	args: &ModeSetArgs,
) -> Result<ExitStatus, CliError> {
	mode_set(ctx, engine, args).await
}

/// Whether the person confirmed what an automatic level can do. `--no-input`
/// can never answer that.
fn acknowledge(ctx: &Ctx, mode: UpdateMode, accept_risks: bool) -> Result<bool, CliError> {
	if !mode.is_automatic() {
		return Ok(false);
	}

	if accept_risks {
		return Ok(true);
	}

	if ctx.no_input || !std::io::IsTerminal::is_terminal(&std::io::stdin()) {
		return Err(CliError::new(
			"CONSENT_REQUIRED",
			ExitStatus::ApprovalRequired,
			"Automatic updates can install a bad release without asking. Pass --accept-risks to confirm you understand.",
		));
	}

	eprintln!("{}", mode.description());

	if crate::prompt::confirm(ctx, "Turn this on?", false)? {
		Ok(true)
	} else {
		Err(CliError::approval("Cancelled. The mode was not changed."))
	}
}

async fn mode_set(
	ctx: &Ctx,
	engine: &Arc<Engine>,
	args: &ModeSetArgs,
) -> Result<ExitStatus, CliError> {
	let mode: UpdateMode = args
		.mode
		.parse()
		.map_err(|_| CliError::usage("The mode must be pinned, manual, on-launch, or daemon."))?;

	let acknowledged = acknowledge(ctx, mode, args.accept_risks)?;

	let previous = engine.update_settings().await.mode;
	let consent = ModeConsent {
		actor: "cli".into(),
		acknowledged_risks: acknowledged,
	};

	// For the background mode this registers the login item first, so the
	// saved mode never claims something the system has not done.
	launcher_background::control::apply_mode(engine, mode, &consent, None)
		.await
		.map_err(crate::background::cli_error)?;

	if ctx.machine() {
		ctx.emit(json!({ "update_mode": mode.as_str(), "previous": previous.as_str(), "started_an_update": false }));
	} else {
		println!("Updates are now: {}", mode.label());

		match mode {
			UpdateMode::OnLaunch => println!(
				"Nothing runs now. Updates will happen the next time the launcher app is opened."
			),
			UpdateMode::Daemon => {
				println!("The background updater is registered to start when you sign in.")
			}
			UpdateMode::Manual => {}
			UpdateMode::Pinned => println!(
				"Apps are held at their versions. Give one its own level with `remade updates policy <app> --mode`."
			),
		}
	}

	Ok(ExitStatus::Success)
}

async fn age_show(ctx: &Ctx, engine: &Arc<Engine>) -> Result<ExitStatus, CliError> {
	let settings = engine.update_settings().await;
	let mut per_app = Vec::new();

	for app in engine.installed().await? {
		if let Some(seconds) = engine.app_update_policy(app.id).await?.min_age_seconds {
			per_app.push((app.name.clone(), seconds));
		}
	}

	if ctx.machine() {
		ctx.emit(json!({
			"minimum_release_age_seconds": settings.minimum_age.seconds(),
			"per_app": per_app.iter().map(|(name, seconds)| json!({ "app": name, "seconds": seconds })).collect::<Vec<_>>(),
			"applies_to": "automatic updates only",
		}));
	} else {
		println!(
			"Minimum release age: {} (automatic updates only)",
			settings.minimum_age.short()
		);

		for (name, seconds) in &per_app {
			println!(
				"  {name}: {}",
				MinimumAge::from_seconds(*seconds)
					.map_or_else(|| format!("{seconds}s"), |age| age.short())
			);
		}

		if settings.minimum_age == MinimumAge::NONE {
			println!(
				"A zero delay installs a release the moment it is published. That is riskier."
			);
		}
	}

	Ok(ExitStatus::Success)
}

async fn age_set(
	ctx: &Ctx,
	engine: &Arc<Engine>,
	duration: &str,
	app: Option<&str>,
) -> Result<ExitStatus, CliError> {
	let age: MinimumAge =
		duration
			.parse()
			.map_err(|error: launcher_model::updates::AgeParseError| {
				CliError::usage(error.to_string())
			})?;

	match app {
		None => engine.set_minimum_age(age).await?,
		Some(reference) => {
			let apps = engine.installed().await?;
			let app = find_app(&apps, reference)?;
			let mut policy = engine.app_update_policy(app.id).await?;

			policy.min_age_seconds = Some(age.seconds());
			engine.set_app_update_policy(app.id, policy).await?;
		}
	}

	if ctx.machine() {
		ctx.emit(
			json!({ "minimum_release_age_seconds": age.seconds(), "app": app, "started_an_update": false }),
		);
	} else {
		println!(
			"Minimum release age{}: {}",
			app.map(|app| format!(" for {app}")).unwrap_or_default(),
			age.short()
		);
	}

	Ok(ExitStatus::Success)
}

async fn age_reset(ctx: &Ctx, engine: &Arc<Engine>, app: &str) -> Result<ExitStatus, CliError> {
	let apps = engine.installed().await?;
	let target = find_app(&apps, app)?;
	let mut policy = engine.app_update_policy(target.id).await?;

	policy.min_age_seconds = None;
	engine.set_app_update_policy(target.id, policy).await?;

	if ctx.machine() {
		ctx.emit(json!({ "app": target.name, "uses_global_default": true }));
	} else {
		println!(
			"{} follows the global minimum release age again.",
			target.name
		);
	}

	Ok(ExitStatus::Success)
}

async fn policy(
	ctx: &Ctx,
	engine: &Arc<Engine>,
	args: &PolicyArgs,
) -> Result<ExitStatus, CliError> {
	let apps = engine.installed().await?;
	let app = find_app(&apps, &args.app)?;
	let mut policy: AppPolicy = engine.app_update_policy(app.id).await?;

	if let Some(level) = &args.mode {
		let chosen = match level.as_str() {
			"default" => None,
			other => Some(other.parse::<UpdateMode>().map_err(|_| {
				CliError::usage("The level must be pinned, manual, on-launch, daemon, or default.")
			})?),
		};
		let acknowledged = match chosen {
			Some(mode) => acknowledge(ctx, mode, args.accept_risks)?,
			None => false,
		};
		let consent = ModeConsent {
			actor: "cli".into(),
			acknowledged_risks: acknowledged,
		};

		engine.set_app_update_mode(app.id, chosen, &consent).await?;

		policy = engine.app_update_policy(app.id).await?;
	}

	let changing = args.pause_auto || args.resume_auto || args.prerelease || args.no_prerelease;

	if args.pause_auto {
		policy.auto_paused = true;
	}

	if args.resume_auto {
		policy.auto_paused = false;
	}

	if args.prerelease {
		policy.allow_prerelease = true;
	}

	if args.no_prerelease {
		policy.allow_prerelease = false;
	}

	if changing {
		engine.set_app_update_policy(app.id, policy.clone()).await?;
	}

	let global = engine.update_settings().await;
	let (age, per_app) = launcher_core::effective_age(&policy, global.minimum_age);
	let level = launcher_core::effective_mode(&policy, global.mode);

	if ctx.machine() {
		ctx.emit(json!({
			"app": app.name,
			"auto_update_paused": policy.auto_paused,
			"pinned": policy.pinned_release_id.is_some(),
			"allow_prereleases": policy.allow_prerelease,
			"effective_minimum_release_age_seconds": age.seconds(),
			"age_source": if per_app { "per_app_override" } else { "global_default" },
			"update_mode": global.mode.as_str(),
			"app_update_mode": policy.update_mode.map(UpdateMode::as_str),
			"effective_update_mode": level.as_str(),
		}));
	} else {
		println!("{}", app.name);
		println!(
			"  Update level:      {}{}",
			level.as_str(),
			if policy.pinned_release_id.is_none() && policy.update_mode.is_none() {
				" (follows the setting for all apps)"
			} else {
				""
			}
		);
		println!(
			"  Automatic updates: {}",
			if policy.auto_paused {
				"paused"
			} else {
				"allowed (when the mode is automatic)"
			}
		);
		println!(
			"  Pinned:            {}",
			if policy.pinned_release_id.is_some() {
				"yes"
			} else {
				"no"
			}
		);
		println!(
			"  Pre-releases:      {}",
			if policy.allow_prerelease {
				"allowed"
			} else {
				"stable only"
			}
		);
		println!(
			"  Minimum age:       {} ({})",
			age.short(),
			if per_app {
				"this app's own"
			} else {
				"global default"
			}
		);
	}

	Ok(ExitStatus::Success)
}

async fn decisions(
	ctx: &Ctx,
	engine: &Arc<Engine>,
	args: &DecisionsArgs,
) -> Result<ExitStatus, CliError> {
	let apps = engine.installed().await?;
	let app = find_app(&apps, &args.app)?;
	let decisions = engine.update_decisions(app.id, args.limit).await?;

	if ctx.machine() {
		ctx.emit(json!({
			"app": app.name,
			"decisions": decisions.iter().map(|decision| json!({
				"when": iso(decision.decided_at), "trigger": decision.trigger, "outcome": decision.outcome,
				"reason": decision.reason, "cycle_id": decision.cycle_id,
			})).collect::<Vec<_>>(),
		}));

		return Ok(ExitStatus::Success);
	}

	if decisions.is_empty() {
		ctx.note("No automatic update has been considered for this app yet.");

		return Ok(ExitStatus::Success);
	}

	let rows: Vec<Vec<String>> = decisions
		.iter()
		.map(|decision| {
			vec![
				when(decision.decided_at),
				decision.trigger.clone(),
				decision.outcome.clone(),
				decision.reason.clone(),
			]
		})
		.collect();

	print_table(&["WHEN", "TRIGGER", "OUTCOME", "REASON"], &rows);

	Ok(ExitStatus::Success)
}
