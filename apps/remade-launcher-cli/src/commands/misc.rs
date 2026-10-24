//! Smaller commands: config, completions, about, self-update, cache, and
//! diagnostics.

use std::sync::Arc;

use clap::CommandFactory;
use launcher_core::Engine;
use launcher_model::local_time::{DateFormat, TimeFormat};
use launcher_model::{
	MinimumAge, Network, Preferences, Proxy, StartPage, ThemePreference, product,
};
use serde_json::json;

use crate::cli::{CacheAction, Cli, ConfigAction};
use crate::context::Ctx;
use crate::errors::CliError;
use crate::exit::ExitStatus;
use crate::output::bytes;

/// Every setting `config` knows, as `(key, value, what it accepts)`.
fn setting_entries(
	theme: ThemePreference,
	settings: &launcher_core::UpdateSettings,
	prefs: &Preferences,
) -> Vec<(&'static str, String, &'static str)> {
	let on = |flag: bool| if flag { "on" } else { "off" }.to_owned();

	vec![
		("theme", theme.as_str().to_owned(), "system, light, or dark"),
		(
			"update.mode",
			settings.mode.as_str().to_owned(),
			"set with `remade updates mode set`",
		),
		(
			"update.minimum_age",
			settings.minimum_age.short(),
			"such as 1h or 24h",
		),
		(
			"language",
			prefs.language.as_str().to_owned(),
			"auto, english, japanese, portuguese_brazil, chinese_simplified, german, or hindi (language tags such as ja or pt-br work too)",
		),
		(
			"region.time_format",
			match prefs.time_format {
				TimeFormat::System => "system",
				TimeFormat::Hour12 => "12h",
				TimeFormat::Hour24 => "24h",
			}
			.to_owned(),
			"system, 12h, or 24h",
		),
		(
			"region.date_format",
			match prefs.date_format {
				DateFormat::System => "system",
				DateFormat::MonthDayYear => "mdy",
				DateFormat::DayMonthYear => "dmy",
				DateFormat::Iso => "iso",
			}
			.to_owned(),
			"system, mdy, dmy, or iso",
		),
		(
			"interface.scale",
			prefs.ui_scale.to_string(),
			"a percentage from 75 to 200",
		),
		(
			"start_page",
			match prefs.start_page {
				StartPage::Auto => "auto",
				StartPage::Library => "library",
				StartPage::Explore => "explore",
				StartPage::LastUsed => "last_used",
			}
			.to_owned(),
			"auto, library, explore, or last_used",
		),
		(
			"notifications.updates",
			on(prefs.notifications.updates_available),
			"on or off",
		),
		(
			"notifications.installs",
			on(prefs.notifications.install_finished),
			"on or off",
		),
		(
			"launcher.updates",
			prefs.launcher_updates.as_str().to_owned(),
			"automatic, notify, or off. Apart from the app update mode",
		),
		(
			"network.proxy",
			match &prefs.network.proxy {
				Proxy::System => "system".to_owned(),
				Proxy::Direct => "direct".to_owned(),
				Proxy::Custom(address) => address.clone(),
			},
			"system, direct, or an http://, https://, or socks5:// address",
		),
		(
			"network.timeout",
			prefs.network.timeout_seconds.to_string(),
			"seconds, from 5 to 300",
		),
	]
}

fn parse_switch(value: &str) -> Result<bool, CliError> {
	match value.to_lowercase().as_str() {
		"on" | "true" | "yes" | "1" => Ok(true),
		"off" | "false" | "no" | "0" => Ok(false),
		_ => Err(CliError::usage("Use on or off.")),
	}
}

/// Applies one `key = value` to a copy of the preferences.
fn change_preference(prefs: &mut Preferences, key: &str, value: &str) -> Result<(), CliError> {
	let wrong = |accepts: &str| CliError::usage(format!("{key} accepts {accepts}."));

	match key {
		"language" => {
			prefs.language = value.parse().map_err(|()| {
				wrong(
					"auto, english, japanese, portuguese_brazil, chinese_simplified, german, or hindi",
				)
			})?;
		}
		"region.time_format" => {
			prefs.time_format = match value.to_lowercase().as_str() {
				"system" => TimeFormat::System,
				"12h" | "12" => TimeFormat::Hour12,
				"24h" | "24" => TimeFormat::Hour24,
				_ => return Err(wrong("system, 12h, or 24h")),
			};
		}
		"region.date_format" => {
			prefs.date_format = match value.to_lowercase().as_str() {
				"system" => DateFormat::System,
				"mdy" => DateFormat::MonthDayYear,
				"dmy" => DateFormat::DayMonthYear,
				"iso" => DateFormat::Iso,
				_ => return Err(wrong("system, mdy, dmy, or iso")),
			};
		}
		"interface.scale" => {
			let scale: u16 = value
				.trim_end_matches('%')
				.parse()
				.map_err(|_| wrong("a percentage such as 110"))?;

			if !(75..=200).contains(&scale) {
				return Err(wrong("a percentage from 75 to 200"));
			}

			prefs.ui_scale = scale;
		}
		"start_page" => {
			prefs.start_page = match value.to_lowercase().as_str() {
				"auto" => StartPage::Auto,
				"library" => StartPage::Library,
				"explore" => StartPage::Explore,
				"last_used" | "last-used" => StartPage::LastUsed,
				_ => return Err(wrong("auto, library, explore, or last_used")),
			};
		}
		"launcher.updates" => {
			prefs.launcher_updates = value
				.parse()
				.map_err(|()| wrong("automatic, notify, or off"))?;
		}
		"notifications.updates" => prefs.notifications.updates_available = parse_switch(value)?,
		"notifications.installs" => prefs.notifications.install_finished = parse_switch(value)?,
		"network.proxy" => {
			prefs.network.proxy = match value.to_lowercase().as_str() {
				"system" => Proxy::System,
				"direct" | "none" => Proxy::Direct,
				_ => {
					launcher_model::preferences::check_proxy(value)
						.map_err(|problem| CliError::usage(problem.message()))?;

					Proxy::Custom(value.trim().to_owned())
				}
			};
		}
		"network.timeout" => {
			let seconds: u32 = value.parse().map_err(|_| wrong("seconds, from 5 to 300"))?;

			if !(Network::MIN_TIMEOUT..=Network::MAX_TIMEOUT).contains(&seconds) {
				return Err(wrong("seconds, from 5 to 300"));
			}

			prefs.network.timeout_seconds = seconds;
		}
		other => {
			return Err(CliError::usage(format!(
				"There is no setting called {other}. See `remade config list`."
			)));
		}
	}

	Ok(())
}

pub(crate) async fn config(
	ctx: &Ctx,
	engine: &Arc<Engine>,
	action: &ConfigAction,
) -> Result<ExitStatus, CliError> {
	let settings = engine.update_settings().await;
	let theme = engine.theme().await;
	let mut prefs = engine.preferences().await;
	let entries = setting_entries(theme, &settings, &prefs);

	match action {
		ConfigAction::List => {
			if ctx.machine() {
				ctx.emit(json!({ "settings": entries.iter().map(|(key, value, accepts)| json!({ "key": key, "value": value, "accepts": accepts })).collect::<Vec<_>>() }));
			} else {
				for (key, value, _) in &entries {
					println!("{key} = {value}");
				}
			}
		}
		ConfigAction::Get { key } => {
			let Some((_, value, _)) = entries.iter().find(|(name, _, _)| name == key) else {
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
					change_preference(&mut prefs, other, value)?;
					engine.set_preferences(prefs).await?;
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

pub(crate) async fn cache(
	ctx: &Ctx,
	engine: &Arc<Engine>,
	action: &CacheAction,
) -> Result<ExitStatus, CliError> {
	match action {
		CacheAction::Status => {
			let status = engine.cache_status().await?;
			let storage = engine.storage_report().await?;

			if ctx.machine() {
				ctx.emit(json!({
					"leftover_folders": status.folders,
					"bytes": status.bytes,
					"remembered_lookups_bytes": storage.cache_bytes,
				}));
			} else {
				if status.folders == 0 {
					println!("Nothing is left over. Installs clean up after themselves.");
				} else {
					println!(
						"{} leftover folder(s), {}. Run `remade cache prune` to remove them.",
						status.folders,
						bytes(status.bytes)
					);
				}

				println!(
					"Remembered GitHub lookups: {}. Run `remade cache clear` to forget them.",
					bytes(storage.cache_bytes)
				);
			}
		}
		CacheAction::Clear => {
			let removed = engine.clear_response_cache().await?;

			if ctx.machine() {
				ctx.emit(json!({ "removed_lookups": removed }));
			} else {
				println!(
					"Forgot {removed} remembered lookup(s). The next check asks GitHub afresh."
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
