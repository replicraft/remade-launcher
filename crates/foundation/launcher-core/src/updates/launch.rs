//! What a genuine launcher start does about updates.

use launcher_model::{LauncherError, UpdateMode};

use super::cycle::{CycleReport, Trigger};
use super::status::UpdateStatus;
use crate::engine::Engine;

/// The result of a launcher start.
#[derive(Debug)]
pub enum LaunchOutcome {
	/// This process already handled its start. Focusing the window, opening
	/// another one, or switching tabs never counts as a new start.
	AlreadyHandled,
	/// Manual mode: a read-only look. Nothing was installed.
	Checked(Vec<UpdateStatus>),
	/// An automatic mode ran its cycle.
	Cycle(CycleReport),
}

impl Engine {
	/// Call exactly once when the graphical launcher starts.
	///
	/// A claim in the database, keyed by this process's session, makes it
	/// safe to call again: only the first call in a session does anything.
	/// The command line never calls this, because running `remade` is not
	/// opening the launcher.
	pub async fn launcher_started(&self) -> Result<LaunchOutcome, LauncherError> {
		let claimed = self
			.database
			.claim_setting("updates.last_gui_session", self.session_id.clone())
			.await?;

		if !claimed {
			return Ok(LaunchOutcome::AlreadyHandled);
		}

		match self.update_settings().await.mode {
			UpdateMode::Manual => Ok(LaunchOutcome::Checked(self.check_updates(None).await?)),
			// In the daemon mode a launcher start also asks for a cycle. If the
			// daemon is already running one, the cycle lock makes this a no-op.
			UpdateMode::OnLaunch => Ok(LaunchOutcome::Cycle(
				self.run_update_cycle(Trigger::OnLaunch).await?,
			)),
			UpdateMode::Daemon => Ok(LaunchOutcome::Cycle(
				self.run_update_cycle(Trigger::Daemon).await?,
			)),
		}
	}
}
