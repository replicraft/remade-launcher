use std::sync::Arc;

use launcher_core::Engine;
use launcher_model::UpdateMode;
use serde_json::json;

use crate::background;
use crate::cli::{DaemonAction, RegisterArgs};
use crate::context::Ctx;
use crate::errors::CliError;
use crate::exit::ExitStatus;
use crate::output::refuse;

pub(crate) async fn run(
	ctx: &Ctx,
	engine: &Arc<Engine>,
	action: &DaemonAction,
) -> Result<ExitStatus, CliError> {
	match action {
		DaemonAction::Status => status(ctx, engine).await,
		DaemonAction::Register(args) => register(ctx, engine, args).await,
		DaemonAction::Unregister => {
			background::unregister(engine).await?;

			// Without a login item the background mode cannot work, so the saved
			// mode must not keep claiming it.
			if engine.update_settings().await.mode == UpdateMode::Daemon {
				let consent = launcher_core::ModeConsent {
					actor: "cli".into(),
					acknowledged_risks: false,
				};

				engine.set_update_mode(UpdateMode::Manual, &consent).await?;
			}

			report(
				ctx,
				"unregistered",
				"The background updater will no longer start at login. Updates are back to manual.",
			)
		}
		DaemonAction::Start => {
			background::start().await?;

			report(ctx, "started", "Asked the system to start the updater.")
		}
		DaemonAction::Stop => {
			let was_running = background::stop(engine).await?;

			if ctx.machine() {
				ctx.emit(json!({ "was_running": was_running, "may_restart_at_next_login": engine.update_settings().await.mode == UpdateMode::Daemon }));
			} else if was_running {
				println!(
					"Stopped the updater. It may start again at your next sign-in; run `remade updates mode set manual` to stop that."
				);
			} else {
				println!("The updater was not running.");
			}

			Ok(ExitStatus::Success)
		}
		DaemonAction::CheckNow => check_now(ctx, engine).await,
		DaemonAction::Logs { tail } => {
			let lines = background::logs(engine, *tail);

			if ctx.machine() {
				ctx.emit(json!({ "lines": lines }));
			} else if lines.is_empty() {
				ctx.note("The updater has not written a log yet.");
			} else {
				for line in lines {
					println!("{line}");
				}
			}

			Ok(ExitStatus::Success)
		}
	}
}

fn report(ctx: &Ctx, result: &str, message: &str) -> Result<ExitStatus, CliError> {
	if ctx.machine() {
		ctx.emit(json!({ "result": result, "message": message }));
	} else {
		println!("{message}");
	}

	Ok(ExitStatus::Success)
}

async fn status(ctx: &Ctx, engine: &Arc<Engine>) -> Result<ExitStatus, CliError> {
	let state = background::describe(engine).await;

	if ctx.machine() {
		ctx.emit(state);

		return Ok(ExitStatus::Success);
	}

	let text = |key: &str| state[key].as_str().unwrap_or("-").to_owned();

	println!("Wanted:      {}", text("desired_state"));
	println!("Actual:      {}", text("actual_state"));
	println!("Mechanism:   {}", text("registration_type"));
	println!("Last check:  {}", text("last_attempted_check"));
	println!("Last result: {}", text("last_result"));
	println!("Next check:  {}", text("next_due_check"));

	if let Some(detail) = state["registration_detail"].as_str() {
		println!("Detail:      {detail}");
	}

	Ok(ExitStatus::Success)
}

async fn register(
	ctx: &Ctx,
	engine: &Arc<Engine>,
	args: &RegisterArgs,
) -> Result<ExitStatus, CliError> {
	// Registering is only half of turning the background mode on, and the
	// other half is a recorded consent. Do both together or neither.
	if !args.accept_risks {
		return Err(CliError::new(
			"CONSENT_REQUIRED",
			ExitStatus::ApprovalRequired,
			"The background updater installs updates without asking. Pass --accept-risks to confirm you understand. This is recorded.",
		));
	}

	let mode_args = crate::cli::ModeSetArgs {
		mode: "daemon".into(),
		accept_risks: true,
	};

	super::updates::mode_set_public(ctx, engine, &mode_args).await
}

async fn check_now(ctx: &Ctx, engine: &Arc<Engine>) -> Result<ExitStatus, CliError> {
	if let Some(detail) = background::check_now(engine).await? {
		if ctx.machine() {
			ctx.emit(json!({ "requested": true, "detail": detail }));
		} else {
			println!("Asked the running updater to look for updates now ({detail}).");
		}

		return Ok(ExitStatus::Success);
	}

	// No updater is running. This is a read-only look, and never turns
	// automatic installing on.
	if ctx.offline {
		return Ok(refuse(
			ctx,
			"OFFLINE",
			ExitStatus::Network,
			"Looking for updates needs the network.",
		));
	}

	ctx.note(
		"The background updater is not running, so this is only a look. Nothing will be installed.",
	);

	super::installed::check(ctx, engine, &crate::cli::CheckArgs { app: None }).await
}
