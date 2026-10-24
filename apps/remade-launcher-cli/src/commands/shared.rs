//! Pieces several commands use.

use std::sync::Arc;

use launcher_core::{
	CatalogState, Decision, Engine, InstalledApp, Origin, ProblemKind, UpdateState, UpdateStatus,
};
use serde_json::{Value, json};

use crate::context::Ctx;
use crate::errors::{CliError, catalog_status};
use crate::exit::ExitStatus;
use crate::output::{date, iso};

/// A cached catalog older than this is refreshed first.
const REFRESH_AFTER_SECONDS: i64 = 24 * 60 * 60;

/// Makes sure there is a verified catalog to work with, refreshing it when it
/// is missing or a day old, and says what is being used.
pub(crate) async fn ensure_catalog(
	ctx: &Ctx,
	engine: &Engine,
) -> Result<Arc<CatalogState>, CliError> {
	let now = time::OffsetDateTime::now_utc().unix_timestamp();
	let state = engine.catalog().state();

	let stale = match (&state.catalog, &state.origin) {
		(None, _) => true,
		(Some(_), Some(Origin::Cache { accepted_at })) => {
			now - accepted_at.unix_timestamp() > REFRESH_AFTER_SECONDS
		}
		_ => false,
	};

	if stale && !ctx.offline {
		engine.catalog().refresh().await;
	}

	let state = engine.catalog().state();

	if let Some(problem) = &state.problem {
		ctx.warn(problem.message.clone());
	}

	if state.clock_warning {
		ctx.warn("Your computer's clock is earlier than a time the launcher already trusted.");
	}

	match (&state.catalog, &state.origin) {
		(Some(_), Some(Origin::Cache { accepted_at })) => {
			ctx.note(format!(
				"Using verified catalog cached on {}.",
				date(accepted_at.unix_timestamp())
			));
		}
		(Some(_), _) => {}
		(None, _) => return Err(no_catalog(&state, ctx.offline)),
	}

	Ok(state)
}

fn no_catalog(state: &CatalogState, offline: bool) -> CliError {
	match &state.problem {
		Some(problem) if problem.kind == ProblemKind::NoTrustedKeys => CliError::new(
			problem.code,
			ExitStatus::Trust,
			"This build has no catalog signing keys, so it cannot verify a catalog.",
		),
		Some(problem) if !offline => CliError::new(
			problem.code,
			catalog_status(problem.code),
			problem.message.clone(),
		),
		_ if offline => CliError::new(
			"CATALOG_UNAVAILABLE_OFFLINE",
			ExitStatus::Network,
			"There is no saved catalog to use offline. Run `remade catalog refresh` when you are online.",
		),
		_ => CliError::new(
			"CATALOG_UNAVAILABLE",
			ExitStatus::Network,
			"No catalog is available.",
		),
	}
}

pub(crate) fn find_app<'a>(
	apps: &'a [InstalledApp],
	reference: &str,
) -> Result<&'a InstalledApp, CliError> {
	crate::app_ref::find(apps, reference).map_err(CliError::from)
}

pub(crate) fn app_json(app: &InstalledApp) -> Value {
	json!({
		"id": app.id.to_string(),
		"short_id": app.id.short(),
		"name": app.name,
		"version": app.version,
		"source": app.repository,
		"reviewed_by_remade": app.curated,
		"catalog_name": app.catalog_slug,
		"installed_at": iso(app.installed_at),
		"launchable": app.launchable,
		"evidence": app.evidence,
	})
}

/// A stable word for what an update lookup found.
pub(crate) fn state_code(state: &UpdateState) -> &'static str {
	match state {
		UpdateState::UpToDate => "up_to_date",
		UpdateState::Available => "available",
		UpdateState::Pinned => "pinned",
		UpdateState::Unsupported(_) => "unsupported",
		UpdateState::CheckFailed(_) => "check_failed",
		UpdateState::NeedsChoice => "needs_choice",
	}
}

pub(crate) fn state_reason(state: &UpdateState) -> Option<&str> {
	match state {
		UpdateState::Unsupported(why) | UpdateState::CheckFailed(why) => Some(why),
		_ => None,
	}
}

pub(crate) fn status_json(status: &UpdateStatus) -> Value {
	let (decision, reason_code, reason) = match &status.auto.decision {
		Decision::Apply => ("apply", Value::Null, Value::Null),
		Decision::Skip(skip) => ("skip", json!(skip.code()), json!(skip.message())),
	};

	let auto_status = match &status.auto.decision {
		Decision::Apply => "eligible",
		Decision::Skip(skip) => skip.status(),
	};

	json!({
		"app": status.name,
		"id": status.installation.to_string(),
		"catalog_name": status.catalog_slug,
		"installed_version": status.installed_version,
		"state": state_code(&status.state),
		"reason": state_reason(&status.state),
		"available": status.available.as_ref().map(|available| json!({
			"version": available.version,
			"file": available.file,
			"size": available.size,
			"published_at": available.published_at.map(iso),
			"rebuilt_artifact": available.rebuilt,
		})),
		"auto_update": {
			"mode": status.auto.mode.as_str(),
			"effective_minimum_release_age_seconds": status.auto.minimum_age.seconds(),
			"source": if status.auto.age_is_per_app { "per_app_override" } else { "global_default" },
			"eligible_at": status.auto.eligible_at.map(iso),
			"decision": decision,
			"status": auto_status,
			"reason": reason_code,
			"explanation": reason,
		},
	})
}

/// A one-line description of an update status for people.
pub(crate) fn status_line(status: &UpdateStatus) -> String {
	match &status.state {
		UpdateState::UpToDate => "up to date".to_owned(),
		UpdateState::Available => {
			let available = status.available.as_ref();
			let version = available.map_or("?", |available| available.version.as_str());

			match &status.auto.decision {
				Decision::Skip(launcher_core::SkipReason::MinReleaseAgeNotMet { eligible_at }) => {
					format!(
						"{version} available (automatic update after {})",
						crate::output::when(*eligible_at)
					)
				}
				_ => format!("{version} available"),
			}
		}
		UpdateState::Pinned => "pinned".to_owned(),
		UpdateState::Unsupported(why) => format!("cannot update: {why}"),
		UpdateState::CheckFailed(why) => format!("could not check: {why}"),
		UpdateState::NeedsChoice => "several packages fit; none chosen".to_owned(),
	}
}
