//! Plans that outlive one command.
//!
//! `plan` writes a sealed plan to the database, and `apply` (possibly from
//! another process, or after the GUI reviewed it) runs exactly that plan. The
//! digest is recomputed on load, so a row edited in place no longer matches
//! and is refused.

use launcher_model::{ErrorCode, InstallationId, LauncherError};
use launcher_resolver::{Candidate, PLAN_LIFETIME_SECONDS, Plan, PlanBody};
use launcher_storage::{ConsentRecord, StoredPlan};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::engine::{Engine, now_unix};
use crate::lifecycle::{InstallMeta, RemovalReport};
use crate::prepare::Ready;

/// What a plan row carries beyond its sealed body.
#[derive(Serialize, Deserialize)]
struct Presentation {
	display_name: String,
	catalog_slug: Option<String>,
	candidate: Candidate,
	notes: Vec<String>,
	unreviewed: bool,
	rebuilt: bool,
	/// For an update: the release that was installed when this was planned.
	installed_release_id: Option<u64>,
}

/// The sealed body of an uninstall plan.
#[derive(Serialize, Deserialize)]
struct UninstallBody {
	schema_version: u32,
	operation: String,
	installation: String,
	name: String,
	repository: String,
	version: String,
	owned_files: usize,
	created_at: i64,
	expires_at: i64,
}

/// A plan, loaded and checked.
#[derive(Debug)]
pub enum SavedPlan {
	Install {
		ready: Box<Ready>,
		update_of: Option<InstallationId>,
		id: String,
	},
	Uninstall(Box<UninstallPlan>),
}

#[derive(Clone, Debug)]
pub struct UninstallPlan {
	pub id: String,
	pub installation: InstallationId,
	pub name: String,
	pub repository: String,
	pub version: String,
	pub owned_files: usize,
	pub expires_at: i64,
	pub(crate) digest: [u8; 32],
}

/// A person's approval of one plan.
#[derive(Clone, Debug)]
pub struct Approval {
	/// Who is approving, such as `cli` or `desktop`.
	pub actor: String,
	/// They reviewed the plan and said go.
	pub confirmed: bool,
	/// For a source Remade has not reviewed: the `owner/repository` they are
	/// accepting. It must name this plan's repository, so approval for one
	/// cannot be reused for another.
	pub accepted_unreviewed_source: Option<String>,
}

#[derive(Debug)]
pub enum Applied {
	Installed(InstallationId),
	Removed(RemovalReport),
}

const PLAN_ID_LENGTH: usize = 16;

fn plan_id(digest: &[u8; 32]) -> String {
	hex::encode(digest).chars().take(PLAN_ID_LENGTH).collect()
}

fn changed(why: &str) -> LauncherError {
	LauncherError::new(ErrorCode::PlanChanged, why.to_owned())
}

impl Engine {
	/// Saves a prepared plan so it can be shown and applied later. Returns
	/// the plan's ID.
	pub async fn save_ready_plan(
		&self,
		ready: &Ready,
		update_of: Option<InstallationId>,
	) -> Result<String, LauncherError> {
		let body = ready.plan.body();
		let installed_release_id = match update_of {
			Some(id) => self
				.database
				.installation(id)
				.await?
				.map(|record| record.release_id),
			None => None,
		};

		let extra = Presentation {
			display_name: ready.meta.display_name.clone(),
			catalog_slug: ready.meta.catalog_slug.clone(),
			candidate: ready.candidate.clone(),
			notes: ready.notes.clone(),
			unreviewed: ready.unreviewed,
			rebuilt: ready.rebuilt,
			installed_release_id,
		};

		let id = plan_id(&ready.plan.digest());

		self.database
			.save_plan(StoredPlan {
				id: id.clone(),
				digest: ready.plan.digest(),
				body: serde_json::to_string(body).map_err(plan_error)?,
				extra: serde_json::to_string(&extra).map_err(plan_error)?,
				update_of,
				created_at: body.created_at,
				expires_at: body.expires_at,
			})
			.await?;

		Ok(id)
	}

	/// Plans removing an installed app. Reads the receipt; changes nothing.
	pub async fn plan_uninstall(
		&self,
		installation: InstallationId,
	) -> Result<UninstallPlan, LauncherError> {
		self.plan_uninstall_at(installation, now_unix()).await
	}

	pub(crate) async fn plan_uninstall_at(
		&self,
		installation: InstallationId,
		now: i64,
	) -> Result<UninstallPlan, LauncherError> {
		let record = self
			.database
			.installation(installation)
			.await?
			.ok_or_else(|| {
				LauncherError::new(
					ErrorCode::InstallationNotFound,
					"That app is not installed.",
				)
			})?;
		let owned_files = serde_json::from_str::<Vec<serde_json::Value>>(&record.manifest)
			.map(|files| files.len())
			.unwrap_or(0);

		let body = UninstallBody {
			schema_version: 1,
			operation: "uninstall".into(),
			installation: installation.to_string(),
			name: record.display_name.clone(),
			repository: record.repository.clone(),
			version: record.release_tag.clone(),
			owned_files,
			created_at: now,
			expires_at: now + PLAN_LIFETIME_SECONDS,
		};

		let value = serde_json::to_value(&body).map_err(plan_error)?;
		let canonical = serde_json_canonicalizer::to_vec(&value).map_err(plan_error)?;
		let digest: [u8; 32] = Sha256::digest(canonical).into();
		let id = plan_id(&digest);

		self.database
			.save_plan(StoredPlan {
				id: id.clone(),
				digest,
				body: value.to_string(),
				extra: "{}".into(),
				update_of: Some(installation),
				created_at: body.created_at,
				expires_at: body.expires_at,
			})
			.await?;

		Ok(UninstallPlan {
			id,
			installation,
			name: body.name,
			repository: body.repository,
			version: body.version,
			owned_files,
			expires_at: body.expires_at,
			digest,
		})
	}

	/// Loads a saved plan, checking that it is intact and has not expired.
	pub async fn load_plan(&self, id: &str) -> Result<SavedPlan, LauncherError> {
		self.load_plan_at(id, now_unix()).await
	}

	pub(crate) async fn load_plan_at(
		&self,
		id: &str,
		now: i64,
	) -> Result<SavedPlan, LauncherError> {
		let stored = self.database.find_plan(id).await?.ok_or_else(|| {
			LauncherError::new(
				ErrorCode::PlanChanged,
				"There is no saved plan with that ID. Make a new one.",
			)
		})?;

		if now >= stored.expires_at {
			return Err(LauncherError::new(
				ErrorCode::PlanExpired,
				"That plan has expired. Make a new one to see the current state.",
			));
		}

		let value: serde_json::Value = serde_json::from_str(&stored.body).map_err(plan_error)?;

		if value["operation"] == "uninstall" {
			return self.load_uninstall(&stored, &value);
		}

		let body: PlanBody = serde_json::from_value(value).map_err(plan_error)?;
		let plan = Plan::seal(body).map_err(plan_error)?;

		if plan.digest() != stored.digest {
			return Err(changed(
				"The saved plan does not match its checksum, so it was not used.",
			));
		}

		let extra: Presentation = serde_json::from_str(&stored.extra).map_err(plan_error)?;

		// An update was planned against a particular installed release. If the
		// app has moved since, the plan describes a state that is gone.
		if let (Some(installation), Some(expected)) = (stored.update_of, extra.installed_release_id)
		{
			let current = self
				.database
				.installation(installation)
				.await?
				.map(|record| record.release_id);

			if current != Some(expected) {
				return Err(changed(
					"The app has changed since this plan was made. Make a new one.",
				));
			}
		}

		let source_display = plan.body().source.display.clone();

		Ok(SavedPlan::Install {
			id: stored.id,
			update_of: stored.update_of,
			ready: Box::new(Ready {
				plan,
				candidate: extra.candidate,
				meta: InstallMeta {
					display_name: extra.display_name,
					catalog_slug: extra.catalog_slug,
				},
				source_display,
				unreviewed: extra.unreviewed,
				notes: extra.notes,
				rebuilt: extra.rebuilt,
			}),
		})
	}

	fn load_uninstall(
		&self,
		stored: &StoredPlan,
		value: &serde_json::Value,
	) -> Result<SavedPlan, LauncherError> {
		let body: UninstallBody = serde_json::from_value(value.clone()).map_err(plan_error)?;
		let canonical = serde_json_canonicalizer::to_vec(value).map_err(plan_error)?;
		let digest: [u8; 32] = Sha256::digest(canonical).into();

		if digest != stored.digest {
			return Err(changed(
				"The saved plan does not match its checksum, so it was not used.",
			));
		}

		let installation = body
			.installation
			.parse()
			.map_err(|_| changed("The saved plan names an app that is not valid."))?;

		Ok(SavedPlan::Uninstall(Box::new(UninstallPlan {
			id: stored.id.clone(),
			installation,
			name: body.name,
			repository: body.repository,
			version: body.version,
			owned_files: body.owned_files,
			expires_at: body.expires_at,
			digest,
		})))
	}

	/// Runs a saved plan, if `approval` covers it.
	pub async fn apply_plan(
		&self,
		id: &str,
		approval: &Approval,
	) -> Result<Applied, LauncherError> {
		self.apply_plan_at(id, approval, now_unix()).await
	}

	pub(crate) async fn apply_plan_at(
		&self,
		id: &str,
		approval: &Approval,
		now: i64,
	) -> Result<Applied, LauncherError> {
		let plan = self.load_plan_at(id, now).await?;

		if !approval.confirmed {
			return Err(LauncherError::new(
				ErrorCode::ConfirmationRequired,
				"This needs your confirmation before it runs.",
			));
		}

		match plan {
			SavedPlan::Install {
				ready, update_of, ..
			} => {
				if ready.unreviewed
					&& approval.accepted_unreviewed_source.as_deref()
						!= Some(ready.source_display.as_str())
				{
					return Err(LauncherError::new(
						ErrorCode::SourceApprovalRequired,
						format!(
							"Remade has not reviewed {}. To install from it, accept that exact repository.",
							ready.source_display
						),
					));
				}

				// A pin holds an app at its release. Putting that same release
				// back, as a repair does, is not leaving it.
				if let Some(installation) = update_of
					&& self
						.database
						.app_policy(installation)
						.await?
						.pinned_release_id
						.is_some_and(|pinned| pinned != ready.candidate.release.id)
				{
					return Err(LauncherError::new(
						ErrorCode::AppPinned,
						"That app is pinned to its current version. Unpin it first.",
					));
				}

				self.record_plan_consent(
					&ready.plan.digest(),
					&ready.source_display,
					approval,
					now,
				)
				.await?;

				let consent = launcher_resolver::Consent {
					source_shown: ready.source_display.clone(),
					acknowledged_unreviewed: ready.unreviewed,
				};
				let approved = ready.plan.clone().approve(&consent, now).map_err(|error| {
					LauncherError::new(ErrorCode::PlanExpired, "The plan could not be approved.")
						.with_source(error)
				})?;

				let installed = match update_of {
					Some(installation) => {
						self.lifecycle()
							.update(installation, &approved, &ready.meta, now)
							.await?
					}
					None => {
						self.lifecycle()
							.install(&approved, &ready.meta, now)
							.await?
					}
				};

				Ok(Applied::Installed(installed))
			}
			SavedPlan::Uninstall(plan) => {
				self.record_plan_consent(&plan.digest, &plan.repository, approval, now)
					.await?;

				Ok(Applied::Removed(
					self.lifecycle().uninstall(plan.installation, now).await?,
				))
			}
		}
	}

	async fn record_plan_consent(
		&self,
		digest: &[u8; 32],
		scope: &str,
		approval: &Approval,
		now: i64,
	) -> Result<(), LauncherError> {
		self.database
			.record_consent(ConsentRecord {
				kind: "plan".into(),
				subject: hex::encode(digest),
				scope: scope.to_owned(),
				policy_version: launcher_model::CONSENT_POLICY_VERSION,
				actor: approval.actor.clone(),
				granted_at: now,
			})
			.await
	}
}

fn plan_error(error: impl std::error::Error + Send + Sync + 'static) -> LauncherError {
	LauncherError::new(
		ErrorCode::PlanChanged,
		"The saved plan could not be read, so it was not used.",
	)
	.with_source(error)
}
