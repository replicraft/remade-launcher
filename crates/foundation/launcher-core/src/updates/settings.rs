//! Global and per-app update settings.

use launcher_model::{
	CONSENT_POLICY_VERSION, ErrorCode, InstallationId, LauncherError, MinimumAge, UpdateMode,
};
use launcher_storage::{AppPolicy, ConsentRecord};

use crate::engine::{Engine, now_unix};

const MODE_KEY: &str = "update.mode";
const AGE_KEY: &str = "update.min_age_seconds";

/// The global update preferences.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UpdateSettings {
	pub mode: UpdateMode,
	pub minimum_age: MinimumAge,
}

/// A person's acknowledgement of what an automatic mode can do.
#[derive(Clone, Debug)]
pub struct ModeConsent {
	/// Who recorded it, such as `cli` or `desktop`.
	pub actor: String,
	/// They read the warning and chose to go ahead. Never pre-filled.
	pub acknowledged_risks: bool,
}

impl Engine {
	/// The saved preferences. Anything unreadable falls back to the safe
	/// defaults: manual updates and a one-hour minimum age.
	pub async fn update_settings(&self) -> UpdateSettings {
		let mode = match self.database.setting(MODE_KEY).await {
			Ok(Some(text)) => text.parse().unwrap_or_default(),
			_ => UpdateMode::default(),
		};
		let minimum_age = match self.database.setting(AGE_KEY).await {
			Ok(Some(text)) => text
				.parse::<u64>()
				.ok()
				.and_then(MinimumAge::from_seconds)
				.unwrap_or_default(),
			_ => MinimumAge::default(),
		};

		UpdateSettings { mode, minimum_age }
	}

	/// Switches the global update mode.
	///
	/// Moving to an automatic mode needs a recorded acknowledgement. This only
	/// changes a setting: it never starts an update cycle, and for the daemon
	/// mode it does not register anything with the operating system.
	pub async fn set_update_mode(
		&self,
		mode: UpdateMode,
		consent: &ModeConsent,
	) -> Result<(), LauncherError> {
		let now = now_unix();

		if mode.is_automatic() {
			if !consent.acknowledged_risks {
				return Err(LauncherError::new(
					ErrorCode::ConsentRequired,
					"Automatic updates can install a bad release without asking. Confirm that you understand before turning them on.",
				));
			}

			self.database
				.record_consent(ConsentRecord {
					kind: "update-mode".into(),
					subject: mode.as_str().into(),
					scope: "this user".into(),
					policy_version: CONSENT_POLICY_VERSION,
					actor: consent.actor.clone(),
					granted_at: now,
				})
				.await?;
		} else {
			for earlier in [UpdateMode::OnLaunch, UpdateMode::Daemon] {
				self.database
					.revoke_consents("update-mode", earlier.as_str(), now)
					.await?;
			}
		}

		self.database
			.set_setting(MODE_KEY, mode.as_str().to_owned())
			.await
	}

	pub async fn set_minimum_age(&self, age: MinimumAge) -> Result<(), LauncherError> {
		self.database
			.set_setting(AGE_KEY, age.seconds().to_string())
			.await
	}

	pub async fn app_update_policy(
		&self,
		installation: InstallationId,
	) -> Result<AppPolicy, LauncherError> {
		self.database.app_policy(installation).await
	}

	pub async fn set_app_update_policy(
		&self,
		installation: InstallationId,
		policy: AppPolicy,
	) -> Result<(), LauncherError> {
		self.require_installed(installation).await?;

		self.database.set_app_policy(installation, policy).await
	}

	/// Why recent automatic updates did or did not happen for one app.
	pub async fn update_decisions(
		&self,
		installation: InstallationId,
		limit: u32,
	) -> Result<Vec<launcher_storage::DecisionRecord>, LauncherError> {
		self.database.recent_decisions(installation, limit).await
	}

	/// Pins an app to the release it has. Pinned apps are not updated
	/// automatically, and not by an operator run either until unpinned.
	pub async fn pin(&self, installation: InstallationId) -> Result<(), LauncherError> {
		let record = self.require_installed(installation).await?;
		let mut policy = self.database.app_policy(installation).await?;

		policy.pinned_release_id = Some(record.release_id);

		self.database.set_app_policy(installation, policy).await
	}

	pub async fn unpin(&self, installation: InstallationId) -> Result<(), LauncherError> {
		self.require_installed(installation).await?;
		let mut policy = self.database.app_policy(installation).await?;

		policy.pinned_release_id = None;

		self.database.set_app_policy(installation, policy).await
	}

	async fn require_installed(
		&self,
		installation: InstallationId,
	) -> Result<launcher_storage::InstallationRecord, LauncherError> {
		self.database
			.installation(installation)
			.await?
			.ok_or_else(|| {
				LauncherError::new(
					ErrorCode::InstallationNotFound,
					"That app is not installed.",
				)
			})
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{DataDirectory, Engine};

	async fn engine() -> (tempfile::TempDir, Engine) {
		let directory = tempfile::tempdir().unwrap();
		let engine = Engine::open(&DataDirectory::at(directory.path().join("data")))
			.await
			.unwrap();

		(directory, engine)
	}

	fn yes() -> ModeConsent {
		ModeConsent {
			actor: "test".into(),
			acknowledged_risks: true,
		}
	}

	#[tokio::test]
	async fn the_defaults_are_manual_and_one_hour() {
		let (_directory, engine) = engine().await;
		let settings = engine.update_settings().await;

		assert_eq!(settings.mode, UpdateMode::Manual);
		assert_eq!(settings.minimum_age.seconds(), 3_600);
	}

	#[tokio::test]
	async fn an_automatic_mode_needs_an_acknowledgement_and_records_it() {
		let (_directory, engine) = engine().await;
		let refuse = ModeConsent {
			actor: "test".into(),
			acknowledged_risks: false,
		};

		let error = engine
			.set_update_mode(UpdateMode::Daemon, &refuse)
			.await
			.unwrap_err();
		assert_eq!(error.code(), ErrorCode::ConsentRequired);
		assert_eq!(engine.update_settings().await.mode, UpdateMode::Manual);

		engine
			.set_update_mode(UpdateMode::OnLaunch, &yes())
			.await
			.unwrap();
		assert_eq!(engine.update_settings().await.mode, UpdateMode::OnLaunch);

		let recorded = engine
			.database
			.consents("update-mode", "on-launch")
			.await
			.unwrap();
		assert_eq!(recorded.len(), 1);
		assert_eq!(recorded[0].policy_version, CONSENT_POLICY_VERSION);
	}

	#[tokio::test]
	async fn going_back_to_manual_withdraws_the_consent_and_needs_none() {
		let (_directory, engine) = engine().await;
		engine
			.set_update_mode(UpdateMode::Daemon, &yes())
			.await
			.unwrap();

		let nothing = ModeConsent {
			actor: "test".into(),
			acknowledged_risks: false,
		};
		engine
			.set_update_mode(UpdateMode::Manual, &nothing)
			.await
			.unwrap();

		assert_eq!(engine.update_settings().await.mode, UpdateMode::Manual);
		assert!(
			engine
				.database
				.consents("update-mode", "daemon")
				.await
				.unwrap()
				.is_empty()
		);
	}

	#[tokio::test]
	async fn the_minimum_age_is_saved_in_seconds() {
		let (_directory, engine) = engine().await;

		engine
			.set_minimum_age("24h".parse().unwrap())
			.await
			.unwrap();

		assert_eq!(engine.update_settings().await.minimum_age.seconds(), 86_400);
		assert_eq!(
			engine.database.setting(AGE_KEY).await.unwrap().as_deref(),
			Some("86400")
		);
	}

	#[tokio::test]
	async fn corrupt_saved_settings_fall_back_to_the_safe_defaults() {
		let (_directory, engine) = engine().await;
		engine
			.database
			.set_setting(MODE_KEY, "yolo".into())
			.await
			.unwrap();
		engine
			.database
			.set_setting(AGE_KEY, "-5".into())
			.await
			.unwrap();

		let settings = engine.update_settings().await;

		assert_eq!(settings.mode, UpdateMode::Manual);
		assert_eq!(settings.minimum_age, MinimumAge::DEFAULT);
	}

	#[tokio::test]
	async fn policy_for_an_app_that_is_not_installed_is_refused() {
		let (_directory, engine) = engine().await;

		let error = engine.pin(InstallationId::new()).await.unwrap_err();

		assert_eq!(error.code(), ErrorCode::InstallationNotFound);
	}
}
