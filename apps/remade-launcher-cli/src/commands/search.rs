use std::process::ExitCode;
use std::sync::Arc;

use launcher_core::{Availability, Engine, ExploreQuery, ExploreRow, State};
use launcher_model::LauncherError;
use serde::Serialize;

use super::ensure_catalog;
use crate::cli::{Cli, SearchArgs};
use crate::output::{print_json, print_table, truncate};

#[derive(Serialize)]
struct Hit<'a> {
	slug: &'a str,
	name: &'a str,
	summary: &'a str,
	source: &'a str,
	status: &'static str,
	for_this_device: bool,
}

pub(crate) async fn run(
	engine: &Arc<Engine>,
	cli: &Cli,
	args: &SearchArgs,
) -> Result<ExitCode, LauncherError> {
	ensure_catalog(engine, cli.offline).await?;

	let rows = engine
		.explore(ExploreQuery {
			text: args.query.join(" "),
			compatible_only: !args.all,
		})
		.await;

	if cli.json {
		let hits: Vec<Hit<'_>> = rows.iter().map(hit).collect();
		print_json(&hits);

		return Ok(ExitCode::SUCCESS);
	}

	if rows.is_empty() {
		eprintln!(
			"No apps match.{}",
			if args.all {
				""
			} else {
				" Add --all to include apps for other devices."
			}
		);

		return Ok(ExitCode::SUCCESS);
	}

	let table: Vec<Vec<String>> = rows
		.iter()
		.map(|row| {
			vec![
				row.slug.clone(),
				truncate(&row.name, 28),
				row.repository.clone(),
				device_cell(row).to_owned(),
				truncate(&row.summary, 60),
			]
		})
		.collect();

	print_table(&["APP", "NAME", "SOURCE", "THIS DEVICE", "SUMMARY"], &table);

	Ok(ExitCode::SUCCESS)
}

fn hit(row: &ExploreRow) -> Hit<'_> {
	Hit {
		slug: &row.slug,
		name: &row.name,
		summary: &row.summary,
		source: &row.repository,
		status: match row.status {
			State::Active => "active",
			State::Paused => "paused",
			State::Revoked => "revoked",
		},
		for_this_device: row.availability == Availability::ThisDevice,
	}
}

fn device_cell(row: &ExploreRow) -> &'static str {
	match &row.availability {
		Availability::ThisDevice => "yes",
		Availability::NeedsNewerLauncher => "newer launcher",
		Availability::OtherPlatforms(_) => "no",
		Availability::NoPackages => "no package",
	}
}
