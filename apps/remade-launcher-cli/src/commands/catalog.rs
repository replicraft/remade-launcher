use std::process::ExitCode;
use std::sync::Arc;

use launcher_core::{Engine, Freshness, Origin};
use launcher_model::LauncherError;
use serde::Serialize;

use crate::cli::{CatalogAction, Cli};
use crate::output::{date, print_json, refuse};

#[derive(Serialize)]
struct Status {
	available: bool,
	revision: Option<u64>,
	apps: Option<usize>,
	source: Option<&'static str>,
	checked: Option<String>,
	expires: Option<String>,
	expired: Option<bool>,
	problem: Option<String>,
}

pub(crate) async fn run(
	engine: &Arc<Engine>,
	cli: &Cli,
	action: &CatalogAction,
) -> Result<ExitCode, LauncherError> {
	if matches!(action, CatalogAction::Refresh) {
		if cli.offline {
			return Ok(refuse("Cannot refresh the catalog with --offline."));
		}

		engine.catalog().refresh().await;
	}

	let state = engine.catalog().state();
	let now = time::OffsetDateTime::now_utc();

	let status = Status {
		available: state.catalog.is_some(),
		revision: state.catalog.as_ref().map(|catalog| catalog.revision()),
		apps: state
			.catalog
			.as_ref()
			.map(|catalog| catalog.project_count()),
		source: state.origin.as_ref().map(|origin| match origin {
			Origin::Network { .. } => "network",
			Origin::Cache { .. } => "cache",
		}),
		checked: state.origin.as_ref().map(|origin| match origin {
			Origin::Network { at } => date(at.unix_timestamp()),
			Origin::Cache { accepted_at } => date(accepted_at.unix_timestamp()),
		}),
		expires: state
			.catalog
			.as_ref()
			.map(|catalog| date(catalog.expires_at().unix_timestamp())),
		expired: state
			.freshness(now)
			.map(|freshness| freshness == Freshness::Expired),
		problem: state
			.problem
			.as_ref()
			.map(|problem| problem.message.clone()),
	};

	if cli.json {
		print_json(&status);
	} else if let (Some(revision), Some(apps)) = (status.revision, status.apps) {
		println!("Verified catalog: revision {revision}, {apps} apps.");
		println!(
			"  From:    {} ({})",
			status.source.unwrap_or("?"),
			status.checked.as_deref().unwrap_or("?")
		);
		println!(
			"  Expires: {}{}",
			status.expires.as_deref().unwrap_or("?"),
			if status.expired == Some(true) {
				" (expired: browsing only)"
			} else {
				""
			}
		);
	} else {
		println!("No verified catalog is available.");
	}

	if !cli.json
		&& let Some(problem) = &status.problem
	{
		eprintln!("warning: {problem}");
	}

	let failed_refresh = matches!(action, CatalogAction::Refresh) && state.problem.is_some();

	Ok(
		if failed_refresh || (!status.available && matches!(action, CatalogAction::Status)) {
			ExitCode::FAILURE
		} else {
			ExitCode::SUCCESS
		},
	)
}
