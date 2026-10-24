use std::process::ExitCode;
use std::sync::Arc;

use launcher_core::Engine;
use launcher_model::LauncherError;
use serde::Serialize;

use crate::app_ref::short_id;
use crate::cli::Cli;
use crate::output::{date, print_json, print_table};

#[derive(Serialize)]
struct Entry<'a> {
	id: String,
	name: &'a str,
	version: &'a str,
	source: &'a str,
	reviewed_by_remade: bool,
	catalog_name: Option<&'a str>,
	installed: String,
	folder: &'a str,
}

pub(crate) async fn run(engine: &Arc<Engine>, cli: &Cli) -> Result<ExitCode, LauncherError> {
	let apps = engine.installed().await?;

	if cli.json {
		let entries: Vec<Entry<'_>> = apps
			.iter()
			.map(|app| Entry {
				id: app.id.to_string(),
				name: &app.name,
				version: &app.version,
				source: &app.repository,
				reviewed_by_remade: app.curated,
				catalog_name: app.catalog_slug.as_deref(),
				installed: date(app.installed_at),
				folder: &app.root,
			})
			.collect();

		print_json(&entries);

		return Ok(ExitCode::SUCCESS);
	}

	if apps.is_empty() {
		eprintln!("Nothing is installed. Try `remade search`.");

		return Ok(ExitCode::SUCCESS);
	}

	let rows: Vec<Vec<String>> = apps
		.iter()
		.map(|app| {
			vec![
				short_id(app),
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

	Ok(ExitCode::SUCCESS)
}
