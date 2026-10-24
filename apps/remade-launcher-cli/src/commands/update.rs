use std::process::ExitCode;
use std::sync::Arc;

use launcher_core::{Engine, InstalledApp, Preparation};
use launcher_model::LauncherError;
use launcher_resolver::Consent;

use super::{ensure_catalog, review};
use crate::cli::{Cli, UpdateArgs};
use crate::output::refuse;
use crate::progress::Printer;
use crate::{app_ref, prompt};

pub(crate) async fn run(
	engine: &Arc<Engine>,
	cli: &Cli,
	args: &UpdateArgs,
) -> Result<ExitCode, LauncherError> {
	if cli.offline {
		return Ok(refuse(
			"Checking for updates needs the network. Run it again without --offline.",
		));
	}

	ensure_catalog(engine, false).await?;

	let apps = engine.installed().await?;
	let targets: Vec<&InstalledApp> = match &args.app {
		Some(reference) => vec![app_ref::find(&apps, reference)?],
		None => apps.iter().filter(|app| app.curated).collect(),
	};

	if targets.is_empty() {
		eprintln!("No catalog apps are installed, so there is nothing to update.");

		return Ok(ExitCode::SUCCESS);
	}

	let mut failures = 0;
	let mut available = 0;

	for app in targets {
		// One app's trouble does not stop the others.
		match engine.prepare_update(app.id).await {
			Ok(Preparation::Ready(ready)) => {
				available += 1;
				println!(
					"{}: {} -> {}",
					app.name, app.version, ready.candidate.release.tag
				);

				if args.check {
					continue;
				}

				review::print(&ready, "Update");

				if !prompt::confirm(&format!("Update {}?", app.name), args.yes)? {
					eprintln!("Skipped {}.", app.name);

					continue;
				}

				let consent = Consent {
					source_shown: ready.source_display.clone(),
					acknowledged_unreviewed: false,
				};
				let printer = Printer::start(engine);
				let result = engine.update(app.id, &ready, &consent).await;

				printer.finish().await;

				match result {
					Ok(_) => println!("Updated {} to {}.", app.name, ready.candidate.release.tag),
					Err(error) => {
						failures += 1;
						eprintln!("{}: update failed. {}", app.name, error.user_message());
					}
				}
			}
			Ok(Preparation::Unavailable(why)) => println!("{}: {why}", app.name),
			Ok(Preparation::Choose(_)) => println!(
				"{}: several packages fit, so no update was chosen.",
				app.name
			),
			Err(error) => {
				failures += 1;
				eprintln!("{}: could not check. {}", app.name, error.user_message());
			}
		}
	}

	if args.check {
		eprintln!("{available} update(s) available.");
	}

	Ok(if failures == 0 {
		ExitCode::SUCCESS
	} else {
		ExitCode::FAILURE
	})
}
