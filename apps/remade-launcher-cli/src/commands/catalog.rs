use std::sync::Arc;

use launcher_core::{Engine, Freshness, Origin};
use serde_json::json;

use super::browse;
use crate::cli::{CatalogAction, ExploreArgs, SlugArgs};
use crate::context::Ctx;
use crate::errors::{CliError, catalog_status};
use crate::exit::ExitStatus;
use crate::output::{date, iso, refuse};

pub(crate) async fn run(
	ctx: &Ctx,
	engine: &Arc<Engine>,
	action: &CatalogAction,
) -> Result<ExitStatus, CliError> {
	match action {
		CatalogAction::Refresh => {
			if ctx.offline {
				return Ok(refuse(
					ctx,
					"OFFLINE",
					ExitStatus::Network,
					"Cannot refresh the catalog with --offline.",
				));
			}

			engine.catalog().refresh().await;

			status(ctx, engine, true)
		}
		CatalogAction::Verify => {
			// Verify the saved copy again, from the bytes, with the trusted keys.
			engine.catalog().load_cached().await;

			status(ctx, engine, true)
		}
		CatalogAction::Status => status(ctx, engine, false),
		CatalogAction::List => browse::explore(ctx, engine, &ExploreArgs { all: true }).await,
		CatalogAction::Show { slug } => {
			browse::info(ctx, engine, &SlugArgs { slug: slug.clone() }).await
		}
	}
}

fn status(ctx: &Ctx, engine: &Arc<Engine>, fail_on_problem: bool) -> Result<ExitStatus, CliError> {
	let state = engine.catalog().state();
	let now = time::OffsetDateTime::now_utc();
	let freshness = state
		.freshness(now)
		.map(|freshness| freshness == Freshness::Expired);
	let source = state.origin.as_ref().map(|origin| match origin {
		Origin::Network { .. } => "network",
		Origin::Cache { .. } => "cache",
	});
	let checked = state.origin.as_ref().map(|origin| match origin {
		Origin::Network { at } => at.unix_timestamp(),
		Origin::Cache { accepted_at } => accepted_at.unix_timestamp(),
	});

	if ctx.machine() {
		ctx.emit(json!({
			"available": state.catalog.is_some(),
			"revision": state.catalog.as_ref().map(|catalog| catalog.revision()),
			"apps": state.catalog.as_ref().map(|catalog| catalog.project_count()),
			"source": source,
			"checked": checked.map(iso),
			"issued": state.catalog.as_ref().map(|catalog| iso(catalog.issued_at().unix_timestamp())),
			"expires": state.catalog.as_ref().map(|catalog| iso(catalog.expires_at().unix_timestamp())),
			"expired": freshness,
			"verified_by": state.catalog.as_ref().map(|catalog| catalog.verified_by().to_vec()),
			"problem": state.problem.as_ref().map(|problem| json!({ "code": problem.code, "message": problem.message })),
		}));
	} else if let Some(catalog) = &state.catalog {
		println!(
			"Verified catalog: revision {}, {} apps.",
			catalog.revision(),
			catalog.project_count()
		);
		println!(
			"  From:      {} ({})",
			source.unwrap_or("?"),
			checked.map(date).unwrap_or_default()
		);
		println!(
			"  Expires:   {}{}",
			date(catalog.expires_at().unix_timestamp()),
			if freshness == Some(true) {
				" (expired: browsing only)"
			} else {
				""
			}
		);
		println!("  Signed by: {}", catalog.verified_by().join(", "));
	} else {
		println!("No verified catalog is available.");
	}

	if !ctx.machine()
		&& let Some(problem) = &state.problem
	{
		eprintln!("warning: {}", problem.message);
	}

	// The status was reported in full above, so a failure is an exit status
	// and not a second error object.
	Ok(match &state.problem {
		Some(problem) if fail_on_problem || state.catalog.is_none() => catalog_status(problem.code),
		None if state.catalog.is_none() => ExitStatus::Network,
		_ => ExitStatus::Success,
	})
}
