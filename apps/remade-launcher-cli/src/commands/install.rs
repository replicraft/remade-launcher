use std::process::ExitCode;
use std::sync::Arc;

use launcher_core::{CustomPreparation, Engine, Preparation, Ready, custom_plan};
use launcher_model::LauncherError;
use launcher_resolver::{Consent, Fit};

use super::{ensure_catalog, review};
use crate::cli::{Cli, InstallArgs};
use crate::output::{bytes, refuse};
use crate::progress::Printer;
use crate::prompt;

pub(crate) async fn run(
	engine: &Arc<Engine>,
	cli: &Cli,
	args: &InstallArgs,
) -> Result<ExitCode, LauncherError> {
	if args.from.is_none() && (args.file.is_some() || args.trust_unreviewed_source) {
		eprintln!("error: --file and --trust-unreviewed-source only apply together with --from");

		return Ok(ExitCode::from(2));
	}

	if cli.offline {
		return Ok(refuse(
			"Installing needs the network. Run it again without --offline.",
		));
	}

	let ready = match (&args.from, &args.slug) {
		(Some(url), _) => match custom(engine, url, args).await? {
			Ok(ready) => ready,
			Err(code) => return Ok(code),
		},
		(None, Some(slug)) => {
			ensure_catalog(engine, false).await?;

			match engine.prepare_install(slug).await? {
				Preparation::Ready(ready) => *ready,
				Preparation::Unavailable(why) => return Ok(refuse(&why)),
				Preparation::Choose(candidates) => {
					eprintln!(
						"Several packages fit this device equally well, and the launcher will not guess:"
					);

					for candidate in &candidates {
						eprintln!("  {} ({})", candidate.asset.name, candidate.variant_id);
					}

					return Ok(refuse(
						"This is a gap in the catalog entry. Nothing was installed.",
					));
				}
			}
		}
		(None, None) => {
			return Ok(refuse(
				"Name an app to install, or use --from with a GitHub link.",
			));
		}
	};

	review::print(&ready, "Install");

	// An unreviewed source needs an explicit statement of trust, even with
	// --yes, so a copied command line cannot skip the warning by accident.
	if ready.unreviewed && !args.trust_unreviewed_source && args.yes {
		return Ok(refuse(
			"Remade has not reviewed this source. Add --trust-unreviewed-source to install it without being asked.",
		));
	}

	let question = if ready.unreviewed {
		format!(
			"Install {} from a source Remade has not reviewed?",
			ready.meta.display_name
		)
	} else {
		format!("Install {}?", ready.meta.display_name)
	};

	if !prompt::confirm(&question, args.yes)? {
		return Ok(refuse("Cancelled."));
	}

	let consent = Consent {
		source_shown: ready.source_display.clone(),
		acknowledged_unreviewed: ready.unreviewed,
	};

	let printer = Printer::start(engine);
	let result = engine.install(&ready, &consent).await;

	printer.finish().await;

	let id = result?;
	let apps = engine.installed().await?;

	if let Some(app) = apps.iter().find(|app| app.id == id) {
		println!("Installed {} {} in {}", app.name, app.version, app.root);

		if app.launchable {
			println!(
				"Start it with: remade run {}",
				app.catalog_slug.as_deref().unwrap_or(&app.name)
			);
		}
	}

	Ok(ExitCode::SUCCESS)
}

/// The `--from` path. The inner `Err` is a refusal that already printed.
async fn custom(
	engine: &Arc<Engine>,
	url: &str,
	args: &InstallArgs,
) -> Result<Result<Ready, ExitCode>, LauncherError> {
	let lookup = engine.prepare_custom(url).await?;

	eprintln!("{} release {}", lookup.identity.display_name(), lookup.tag);

	if !lookup.discovery_complete {
		eprintln!("Only the most recent releases were checked.");
	}

	let Some(asset_id) = pick_file(&lookup, args.file.as_deref()) else {
		return Ok(Err(list_options(&lookup, args.file.as_deref())));
	};

	let now = time::OffsetDateTime::now_utc().unix_timestamp();

	custom_plan(&lookup, asset_id, now).map(Ok)
}

/// The named file, or the one obvious choice. Never the first of several.
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

fn list_options(lookup: &CustomPreparation, wanted: Option<&str>) -> ExitCode {
	if let Some(name) = wanted {
		eprintln!("This release has no file named \"{name}\".");
	} else {
		eprintln!("The launcher cannot tell which file to install. Choose one with --file:");
	}

	for option in &lookup.options {
		let note = match (option.managed, option.fit) {
			(false, _) => "download only",
			(true, Fit::Matches) => "named for this device",
			(true, Fit::Unknown) => "name does not say which device",
			(true, Fit::Mismatch) => "named for another device",
		};

		eprintln!(
			"  {} ({}, {note})",
			option.asset.name,
			bytes(option.asset.size)
		);
	}

	ExitCode::FAILURE
}
