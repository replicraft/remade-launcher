mod catalog;
mod history;
mod info;
mod install;
mod list;
mod review;
mod run;
mod search;
mod uninstall;
mod update;

use std::process::ExitCode;
use std::sync::Arc;

use launcher_core::{CatalogState, Engine, Origin, ProblemKind};
use launcher_model::{ErrorCode, LauncherError};

use crate::cli::{Cli, Command};

pub(crate) async fn dispatch(engine: &Arc<Engine>, cli: &Cli) -> Result<ExitCode, LauncherError> {
	match &cli.command {
		Command::Search(args) => search::run(engine, cli, args).await,
		Command::Info(args) => info::run(engine, cli, args).await,
		Command::List => list::run(engine, cli).await,
		Command::History(args) => history::run(engine, cli, args).await,
		Command::Install(args) => install::run(engine, cli, args).await,
		Command::Update(args) => update::run(engine, cli, args).await,
		Command::Uninstall(args) => uninstall::run(engine, args).await,
		Command::Run(args) => run::run(engine, args).await,
		Command::Catalog { action } => catalog::run(engine, cli, action).await,
	}
}

/// A cached catalog older than this is refreshed first.
const REFRESH_AFTER_SECONDS: i64 = 24 * 60 * 60;

/// Makes sure there is a verified catalog to work with, refreshing it when it
/// is missing or a day old, and says on stderr what is being used.
pub(crate) async fn ensure_catalog(
	engine: &Engine,
	offline: bool,
) -> Result<Arc<CatalogState>, LauncherError> {
	let now = time::OffsetDateTime::now_utc().unix_timestamp();
	let state = engine.catalog().state();

	let stale = match (&state.catalog, &state.origin) {
		(None, _) => true,
		(Some(_), Some(Origin::Cache { accepted_at })) => {
			now - accepted_at.unix_timestamp() > REFRESH_AFTER_SECONDS
		}
		_ => false,
	};

	if stale && !offline {
		engine.catalog().refresh().await;
	}

	let state = engine.catalog().state();

	if let Some(problem) = &state.problem {
		eprintln!("warning: {}", problem.message);
	}

	if state.clock_warning {
		eprintln!(
			"warning: your computer's clock is earlier than a time the launcher already trusted."
		);
	}

	match (&state.catalog, &state.origin) {
		(Some(_), Some(Origin::Cache { accepted_at })) => {
			eprintln!(
				"Using verified catalog cached on {}.",
				crate::output::date(accepted_at.unix_timestamp())
			);
		}
		(Some(_), _) => {}
		(None, _) => {
			let why = match state.problem.as_ref().map(|problem| problem.kind) {
				Some(ProblemKind::NoTrustedKeys) => {
					"This build has no catalog signing keys, so it cannot verify a catalog."
				}
				Some(_) if offline => "There is no saved catalog to use offline.",
				Some(_) => "The catalog could not be loaded or verified.",
				None if offline => {
					"There is no saved catalog to use offline. Run `remade catalog refresh`."
				}
				None => "No catalog is available.",
			};

			return Err(LauncherError::new(ErrorCode::NetworkRequestFailed, why));
		}
	}

	Ok(state)
}
