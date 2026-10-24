//! The tools.

use std::sync::Arc;

use launcher_core::{
	Applied, Approval, Engine, ExploreQuery, InstalledApp, Preparation, Ready, RepairOutcome,
	SavedPlan, UpdateState, UpdateStatus,
};
use launcher_model::ErrorCode;
use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{ServerCapabilities, ServerConfig};
use rmcp::{ServerHandler, tool, tool_handler, tool_router};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::Permissions;

#[derive(Clone)]
pub(crate) struct Remade {
	engine: Arc<Engine>,
	permissions: Permissions,
	tool_router: ToolRouter<Self>,
}

#[derive(Deserialize, JsonSchema)]
pub(crate) struct SearchArgs {
	/// Words to look for in names, summaries, categories, and repositories.
	/// Leave it empty to browse everything.
	#[serde(default)]
	query: String,
	/// Include apps with no package for this computer.
	#[serde(default)]
	include_incompatible: bool,
}

#[derive(Deserialize, JsonSchema)]
pub(crate) struct AppArgs {
	/// An app's catalog name, such as `sample-editor`.
	name: String,
}

#[derive(Deserialize, JsonSchema)]
pub(crate) struct InstalledArgs {
	/// An installed app's name, catalog name, or short ID.
	app: String,
}

#[derive(Deserialize, JsonSchema)]
pub(crate) struct ChangelogArgs {
	/// An app's catalog name.
	name: String,
	/// A release tag. The newest eligible release if left out.
	tag: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
pub(crate) struct PlanArgs {
	/// The plan's ID, from one of the plan tools.
	plan_id: String,
}

#[derive(Deserialize, JsonSchema)]
pub(crate) struct ApplyArgs {
	/// The plan's ID.
	plan_id: String,
	/// The app's name exactly as the plan shows it.
	app: String,
	/// The version exactly as the plan shows it.
	version: String,
}

fn text(value: &Value) -> String {
	serde_json::to_string_pretty(value).unwrap_or_default()
}

fn failure(error: impl std::fmt::Display) -> String {
	error.to_string()
}

/// UTC, whole seconds, with a `Z`: the form every timestamp here uses.
fn iso(unix: i64) -> String {
	let time =
		time::OffsetDateTime::from_unix_timestamp(unix).unwrap_or(time::OffsetDateTime::UNIX_EPOCH);

	format!(
		"{}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
		time.year(),
		u8::from(time.month()),
		time.day(),
		time.hour(),
		time.minute(),
		time.second()
	)
}

fn plan_value(id: &str, ready: &Ready) -> Value {
	let body = ready.plan.body();

	json!({
		"plan_id": id,
		"operation": body.operation.verb(),
		"app": ready.meta.display_name,
		"version": body.release.tag,
		"source": ready.source_display,
		"reviewed_by_remade": !ready.unreviewed,
		"file": { "name": body.asset.name, "size": body.asset.size },
		"expires_at": iso(body.expires_at),
		"notes": ready.notes,
		"next": "A person must approve this before it can be applied.",
	})
}

fn state_code(state: &UpdateState) -> &'static str {
	match state {
		UpdateState::UpToDate => "up_to_date",
		UpdateState::Available => "update_available",
		UpdateState::Pinned => "pinned",
		UpdateState::Unsupported(_) => "unsupported",
		UpdateState::CheckFailed(_) => "check_failed",
		UpdateState::NeedsChoice => "needs_choice",
	}
}

fn status_value(status: &UpdateStatus) -> Value {
	json!({
		"app": status.name,
		"installed_version": status.installed_version,
		"state": state_code(&status.state),
		"available_version": status.available.as_ref().map(|update| &update.version),
		"detail": match &status.state {
			UpdateState::Unsupported(why) | UpdateState::CheckFailed(why) => Some(why.as_str()),
			_ => None,
		},
	})
}

fn installed_value(app: &InstalledApp) -> Value {
	json!({
		"id": app.id.short(),
		"name": app.name,
		"version": app.version,
		"source": app.repository,
		"reviewed_by_remade": app.curated,
		"catalog_name": app.catalog_slug,
	})
}

/// The installed app a reference names, or why none does.
fn find<'a>(apps: &'a [InstalledApp], reference: &str) -> Result<&'a InstalledApp, String> {
	let wanted = reference.trim().to_lowercase();
	let matching: Vec<&InstalledApp> = apps
		.iter()
		.filter(|app| {
			app.id.matches(&wanted)
				|| app
					.catalog_slug
					.as_deref()
					.is_some_and(|slug| slug.eq_ignore_ascii_case(&wanted))
				|| app.name.to_lowercase() == wanted
		})
		.collect();

	match matching.as_slice() {
		[only] => Ok(only),
		[] => Err(format!("No installed app matches \"{reference}\".")),
		_ => Err(format!(
			"\"{reference}\" matches more than one app. Use its ID from list_installed."
		)),
	}
}

/// What a saved plan is, for deciding whether an assistant may apply it.
struct PlanFacts<'a> {
	name: &'a str,
	version: &'a str,
	destructive: bool,
	unreviewed: bool,
}

/// Whether an assistant that names `app` and `version` may apply a plan with
/// these facts. It may only approve exactly what it showed, never an app from
/// an unreviewed source, and never a removal unless removals were allowed.
fn authorize(
	permissions: Permissions,
	plan: &PlanFacts<'_>,
	app: &str,
	version: &str,
) -> Result<(), String> {
	if !permissions.apply {
		return Err("This server was started without permission to make changes. Ask the person to run the plan with the remade command, or to restart it with --allow-apply.".to_owned());
	}

	if !plan.name.eq_ignore_ascii_case(app) || plan.version != version {
		return Err(format!(
			"That does not match the plan, which is for {} {}. Show the plan and use its exact app and version.",
			plan.name, plan.version
		));
	}

	if plan.unreviewed {
		return Err("That app comes from a source Remade has not reviewed. Only a person can accept that, at the command line.".to_owned());
	}

	if plan.destructive && !permissions.destructive {
		return Err(
			"Removing apps needs --allow-destructive, which this server was not started with."
				.to_owned(),
		);
	}

	Ok(())
}

#[tool_router(router = tool_router)]
impl Remade {
	pub(crate) fn new(engine: Arc<Engine>, permissions: Permissions) -> Self {
		Self {
			engine,
			permissions,
			tool_router: Self::tool_router(),
		}
	}

	async fn catalog_ready(&self) -> Result<(), String> {
		if self.engine.catalog().state().catalog.is_none() {
			self.engine.catalog().refresh().await;
		}

		if self.engine.catalog().state().catalog.is_none() {
			return Err("There is no verified catalog available right now.".to_owned());
		}

		Ok(())
	}

	#[tool(description = "Search the verified Remade catalog. Read-only.")]
	async fn search_catalog(&self, params: Parameters<SearchArgs>) -> Result<String, String> {
		self.catalog_ready().await?;

		let rows = self
			.engine
			.explore(ExploreQuery {
				text: params.0.query,
				compatible_only: !params.0.include_incompatible,
			})
			.await;

		Ok(text(&json!({
			"apps": rows.iter().take(50).map(|row| json!({
				"name": row.slug,
				"title": row.name,
				"summary": row.summary,
				"source": row.repository,
				"has_package_for_this_computer": row.availability == launcher_core::Availability::ThisDevice,
			})).collect::<Vec<_>>(),
			"total_matches": rows.len(),
		})))
	}

	#[tool(description = "Everything the catalog says about one app. Read-only.")]
	async fn app_info(&self, params: Parameters<AppArgs>) -> Result<String, String> {
		self.catalog_ready().await?;

		let state = self.engine.catalog().state();
		let project = state
			.catalog
			.as_ref()
			.and_then(|catalog| catalog.project(&params.0.name))
			.ok_or_else(|| format!("\"{}\" is not in the catalog.", params.0.name))?;
		let report = self
			.engine
			.platforms(Some(&params.0.name))
			.map_err(failure)?;
		let supported = report
			.apps
			.first()
			.is_some_and(|(_, variants)| variants.iter().any(|variant| variant.supported));

		Ok(text(&json!({
			"name": params.0.name,
			"title": project.display.name,
			"summary": project.display.summary,
			"description": project.display.description,
			"source": project.source.display_name(),
			"categories": project.display.categories,
			"languages": project.display.languages,
			"license": project.display.license,
			"has_package_for_this_computer": supported,
		})))
	}

	#[tool(
		description = "The apps installed on this computer by the launcher, and apps it only watches. Read-only."
	)]
	async fn list_installed(&self) -> Result<String, String> {
		let apps = self.engine.installed().await.map_err(failure)?;
		let watched = self.engine.adopted_apps().await.map_err(failure)?;

		Ok(text(&json!({
			"managed": apps.iter().map(installed_value).collect::<Vec<_>>(),
			"watched_not_managed": watched.iter().map(|app| json!({ "name": app.name, "present": app.present })).collect::<Vec<_>>(),
		})))
	}

	#[tool(description = "Look for updates to installed apps. Read-only: it installs nothing.")]
	async fn check_updates(&self) -> Result<String, String> {
		self.catalog_ready().await?;

		let statuses = self.engine.check_updates(None).await.map_err(failure)?;

		Ok(text(
			&json!({ "apps": statuses.iter().map(status_value).collect::<Vec<_>>() }),
		))
	}

	#[tool(description = "Release notes for an app, as plain text. Read-only.")]
	async fn changelog(&self, params: Parameters<ChangelogArgs>) -> Result<String, String> {
		self.catalog_ready().await?;

		let notes = self
			.engine
			.changelog(&params.0.name, params.0.tag.as_deref())
			.await
			.map_err(failure)?;

		Ok(text(
			&json!({ "version": notes.version, "notes": notes.text }),
		))
	}

	#[tool(
		description = "Plan installing a catalog app. Changes nothing. Returns a plan for a person to approve."
	)]
	async fn plan_install(&self, params: Parameters<AppArgs>) -> Result<String, String> {
		self.catalog_ready().await?;

		match self
			.engine
			.prepare_install(&params.0.name)
			.await
			.map_err(failure)?
		{
			Preparation::Ready(ready) => {
				let id = self
					.engine
					.save_ready_plan(&ready, None)
					.await
					.map_err(failure)?;

				Ok(text(&plan_value(&id, &ready)))
			}
			Preparation::Unavailable(why) => Err(why),
			Preparation::Choose(_) => Err(
				"Several packages fit and the launcher will not guess which. A person must choose."
					.to_owned(),
			),
		}
	}

	#[tool(description = "Plan updating an installed app. Changes nothing.")]
	async fn plan_update(&self, params: Parameters<InstalledArgs>) -> Result<String, String> {
		self.catalog_ready().await?;

		let apps = self.engine.installed().await.map_err(failure)?;
		let app = find(&apps, &params.0.app)?;

		match self.engine.prepare_update(app.id).await.map_err(failure)? {
			Preparation::Ready(ready) => {
				let id = self
					.engine
					.save_ready_plan(&ready, Some(app.id))
					.await
					.map_err(failure)?;

				Ok(text(&plan_value(&id, &ready)))
			}
			Preparation::Unavailable(why) => Err(why),
			Preparation::Choose(_) => Err(
				"Several packages fit and the launcher will not guess which. A person must choose."
					.to_owned(),
			),
		}
	}

	#[tool(
		description = "Check an installed app's files against what was installed, and plan putting it back if it is damaged. Changes nothing."
	)]
	async fn plan_repair(&self, params: Parameters<InstalledArgs>) -> Result<String, String> {
		let apps = self.engine.installed().await.map_err(failure)?;
		let app = find(&apps, &params.0.app)?;

		self.catalog_ready().await?;

		let assessment = self.engine.assess_repair(app.id).await.map_err(failure)?;
		let diagnosis = json!({
			"clean": assessment.report.is_clean(),
			"missing": assessment.report.missing,
			"modified": assessment.report.modified,
			"problems": assessment.report.problems,
		});

		match assessment.outcome {
			RepairOutcome::NotNeeded => Ok(text(
				&json!({ "repair": "not_needed", "diagnosis": diagnosis }),
			)),
			RepairOutcome::Unavailable(why) => Err(why),
			RepairOutcome::Ready(ready) => {
				let id = self
					.engine
					.save_ready_plan(&ready, Some(app.id))
					.await
					.map_err(failure)?;

				Ok(text(
					&json!({ "diagnosis": diagnosis, "plan": plan_value(&id, &ready) }),
				))
			}
		}
	}

	#[tool(
		description = "Plan removing an installed app. Changes nothing. Files the person edited or the app saved are kept."
	)]
	async fn plan_uninstall(&self, params: Parameters<InstalledArgs>) -> Result<String, String> {
		let apps = self.engine.installed().await.map_err(failure)?;
		let app = find(&apps, &params.0.app)?;
		let plan = self.engine.plan_uninstall(app.id).await.map_err(failure)?;

		Ok(text(&json!({
			"plan_id": plan.id,
			"operation": "uninstall",
			"app": plan.name,
			"version": plan.version,
			"removes_up_to_files": plan.owned_files,
			"expires_at": iso(plan.expires_at),
			"next": "A person must approve this before it can be applied.",
		})))
	}

	#[tool(description = "Show a saved plan again. Read-only.")]
	async fn show_plan(&self, params: Parameters<PlanArgs>) -> Result<String, String> {
		match self
			.engine
			.load_plan(&params.0.plan_id)
			.await
			.map_err(failure)?
		{
			SavedPlan::Install { ready, id, .. } => Ok(text(&plan_value(&id, &ready))),
			SavedPlan::Uninstall(plan) => Ok(text(&json!({
				"plan_id": plan.id, "operation": "uninstall", "app": plan.name, "version": plan.version,
			}))),
		}
	}

	#[tool(
		description = "Apply a saved plan. Only works if the person started this server allowing it, and only for the exact plan, app, and version given."
	)]
	async fn apply_plan(&self, params: Parameters<ApplyArgs>) -> Result<String, String> {
		if !self.permissions.apply {
			return Err("This server was started without permission to make changes. Ask the person to run the plan with the remade command, or to restart it with --allow-apply.".to_owned());
		}

		let args = params.0;
		let saved = self
			.engine
			.load_plan(&args.plan_id)
			.await
			.map_err(failure)?;

		let (name, version, destructive, unreviewed) = match &saved {
			SavedPlan::Install { ready, .. } => (
				ready.meta.display_name.clone(),
				ready.plan.body().release.tag.clone(),
				false,
				ready.unreviewed,
			),
			SavedPlan::Uninstall(plan) => (plan.name.clone(), plan.version.clone(), true, false),
		};

		authorize(
			self.permissions,
			&PlanFacts {
				name: &name,
				version: &version,
				destructive,
				unreviewed,
			},
			&args.app,
			&args.version,
		)?;

		let approval = Approval {
			actor: "mcp".to_owned(),
			confirmed: true,
			accepted_unreviewed_source: None,
		};

		match self.engine.apply_plan(&args.plan_id, &approval).await {
			Ok(Applied::Installed(id)) => Ok(text(
				&json!({ "result": "completed", "installation": id.short() }),
			)),
			Ok(Applied::Removed(report)) => Ok(text(&json!({
				"result": "completed",
				"files_removed": report.removed_files,
				"kept_because_edited": report.kept_modified,
				"kept_unknown": report.kept_unknown,
			}))),
			Err(error) if error.code() == ErrorCode::PlanExpired => {
				Err("That plan has expired. Make a new one.".to_owned())
			}
			Err(error) => Err(failure(error.user_message())),
		}
	}
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for Remade {
	fn get_info(&self) -> ServerConfig {
		ServerConfig::new(ServerCapabilities::builder().enable_tools().build()).with_instructions(
			"Remade Launcher installs and updates apps from a verified catalog. You can search, inspect, and plan. Applying a plan needs the person's permission, and a source Remade has not reviewed always needs a person at the command line.",
		)
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	const ALLOWED: Permissions = Permissions {
		apply: true,
		destructive: false,
	};
	const EVERYTHING: Permissions = Permissions {
		apply: true,
		destructive: true,
	};

	fn plan(destructive: bool, unreviewed: bool) -> PlanFacts<'static> {
		PlanFacts {
			name: "Quill",
			version: "v1.2.0",
			destructive,
			unreviewed,
		}
	}

	#[test]
	fn nothing_applies_without_permission() {
		assert!(
			authorize(
				Permissions::default(),
				&plan(false, false),
				"Quill",
				"v1.2.0"
			)
			.is_err()
		);
	}

	#[test]
	fn the_assistant_must_name_exactly_what_the_plan_does() {
		assert!(
			authorize(ALLOWED, &plan(false, false), "quill", "v1.2.0").is_ok(),
			"case does not matter"
		);
		assert!(
			authorize(ALLOWED, &plan(false, false), "Quill", "v1.3.0").is_err(),
			"another version"
		);
		assert!(
			authorize(ALLOWED, &plan(false, false), "Other", "v1.2.0").is_err(),
			"another app"
		);
		assert!(
			authorize(ALLOWED, &plan(false, false), "", "").is_err(),
			"nothing at all"
		);
	}

	#[test]
	fn an_unreviewed_source_is_never_applied_by_an_assistant() {
		assert!(authorize(EVERYTHING, &plan(false, true), "Quill", "v1.2.0").is_err());
	}

	#[test]
	fn removals_need_their_own_permission() {
		assert!(authorize(ALLOWED, &plan(true, false), "Quill", "v1.2.0").is_err());
		assert!(authorize(EVERYTHING, &plan(true, false), "Quill", "v1.2.0").is_ok());
	}
}
