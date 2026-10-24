//! Smaller commands: config, completions, about, self-update, cache, and
//! diagnostics.

use std::sync::Arc;

use clap::CommandFactory;
use launcher_core::Engine;
use launcher_model::{MinimumAge, ThemePreference, product};
use serde_json::json;

use crate::cli::{CacheAction, Cli, ConfigAction, SelfUpdateAction};
use crate::context::Ctx;
use crate::errors::CliError;
use crate::exit::ExitStatus;
use crate::output::{bytes, refuse};

pub(crate) async fn config(
	ctx: &Ctx,
	engine: &Arc<Engine>,
	action: &ConfigAction,
) -> Result<ExitStatus, CliError> {
	let settings = engine.update_settings().await;
	let theme = engine.theme().await;

	let entries = [
		("theme", theme.as_str().to_owned()),
		("update.mode", settings.mode.as_str().to_owned()),
		("update.minimum_age", settings.minimum_age.short()),
	];

	match action {
		ConfigAction::List => {
			if ctx.machine() {
				ctx.emit(json!({ "settings": entries.iter().map(|(key, value)| json!({ "key": key, "value": value })).collect::<Vec<_>>() }));
			} else {
				for (key, value) in &entries {
					println!("{key} = {value}");
				}
			}
		}
		ConfigAction::Get { key } => {
			let Some((_, value)) = entries.iter().find(|(name, _)| name == key) else {
				return Err(CliError::usage(format!(
					"There is no setting called {key}. See `remade config list`."
				)));
			};

			if ctx.machine() {
				ctx.emit(json!({ "key": key, "value": value }));
			} else {
				println!("{value}");
			}
		}
		ConfigAction::Set { key, value } => {
			match key.as_str() {
				"theme" => {
					let theme: ThemePreference = value.parse().map_err(|_| {
						CliError::usage("The theme must be system, light, or dark.")
					})?;

					engine.set_theme(theme).await?;
				}
				"update.minimum_age" => {
					let age: MinimumAge = value.parse().map_err(
						|error: launcher_model::updates::AgeParseError| {
							CliError::usage(error.to_string())
						},
					)?;

					engine.set_minimum_age(age).await?;
				}
				"update.mode" => {
					return Err(CliError::usage(
						"The update mode needs a recorded acknowledgement. Use `remade updates mode set`.",
					));
				}
				other => {
					return Err(CliError::usage(format!(
						"There is no setting called {other}. See `remade config list`."
					)));
				}
			}

			if ctx.machine() {
				ctx.emit(json!({ "key": key, "value": value }));
			} else {
				println!("{key} = {value}");
			}
		}
	}

	Ok(ExitStatus::Success)
}

pub(crate) fn completions(shell: clap_complete::Shell) -> ExitStatus {
	let mut command = Cli::command();

	clap_complete::generate(shell, &mut command, "remade", &mut std::io::stdout());

	ExitStatus::Success
}

pub(crate) fn about(ctx: &Ctx, engine: &Arc<Engine>) -> ExitStatus {
	let trust = engine.trust_status();
	let build = if cfg!(debug_assertions) {
		"debug"
	} else {
		"release"
	};

	if ctx.machine() {
		ctx.emit(json!({
			"name": product::NAME,
			"version": product::VERSION,
			"tagline": product::TAGLINE,
			"application_id": product::APP_ID,
			"catalog_feed": trust.feed_id,
			"trusted_catalog_keys": trust.trusted_key_ids.len(),
			"os": std::env::consts::OS,
			"architecture": std::env::consts::ARCH,
			"build": build,
		}));
	} else {
		println!("{} {}", product::NAME, product::VERSION);
		println!("{}", product::TAGLINE);
		println!(
			"  System:        {} {}",
			std::env::consts::OS,
			std::env::consts::ARCH
		);
		println!("  Build:         {build}");
		println!("  Catalog feed:  {}", trust.feed_id);
		println!("  Catalog keys:  {} trusted", trust.trusted_key_ids.len());
	}

	ExitStatus::Success
}

pub(crate) fn self_update(ctx: &Ctx, action: &SelfUpdateAction) -> ExitStatus {
	let reason = "Launcher updates come through a separate signed pipeline that is not available yet. Install a newer launcher from the project's releases page.";

	match action {
		SelfUpdateAction::Status => {
			if ctx.machine() {
				ctx.emit(
					json!({ "available": false, "reason": reason, "current_version": product::VERSION }),
				);
			} else {
				println!("{} {}", product::NAME, product::VERSION);
				println!("{reason}");
			}

			ExitStatus::Success
		}
		SelfUpdateAction::Check | SelfUpdateAction::Apply => refuse(
			ctx,
			"SELF_UPDATE_UNAVAILABLE",
			ExitStatus::Unsupported,
			reason,
		),
	}
}

pub(crate) async fn cache(
	ctx: &Ctx,
	engine: &Arc<Engine>,
	action: &CacheAction,
) -> Result<ExitStatus, CliError> {
	match action {
		CacheAction::Status => {
			let status = engine.cache_status().await?;

			if ctx.machine() {
				ctx.emit(json!({ "leftover_folders": status.folders, "bytes": status.bytes }));
			} else if status.folders == 0 {
				println!("Nothing is left over. Installs clean up after themselves.");
			} else {
				println!(
					"{} leftover folder(s), {}. Run `remade cache prune` to remove them.",
					status.folders,
					bytes(status.bytes)
				);
			}
		}
		CacheAction::Prune => {
			let removed = engine.prune_cache().await?;

			if ctx.machine() {
				ctx.emit(json!({ "removed_folders": removed }));
			} else {
				println!("Removed {removed} leftover folder(s).");
			}
		}
	}

	Ok(ExitStatus::Success)
}

pub(crate) async fn diagnostics(ctx: &Ctx, engine: &Arc<Engine>) -> Result<ExitStatus, CliError> {
	let report = engine.diagnostics().await?;

	if ctx.machine() {
		ctx.emit(json!({ "report": report }));
	} else {
		print!("{report}");
	}

	Ok(ExitStatus::Success)
}
