//! What a genuine launcher start does about updates.

use launcher_model::LauncherError;

use super::cycle::{CycleReport, Trigger};
use super::status::UpdateStatus;
use crate::engine::Engine;

/// The result of a launcher start.
#[derive(Debug)]
pub enum LaunchOutcome {
	/// This process already handled its start. Focusing the window, opening
	/// another one, or switching tabs never counts as a new start.
	AlreadyHandled,
	/// Nothing is set to update on its own: a read-only look. Nothing was
	/// installed.
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

		// An app can choose its own level, so one set to update at launch is
		// enough even when the global mode is Manual. The cycle also covers
		// what the background updater would do. If it is already running one,
		// the cycle lock makes this a no-op.
		if self.any_app_allows(Trigger::OnLaunch).await? {
			Ok(LaunchOutcome::Cycle(
				self.run_update_cycle(Trigger::OnLaunch).await?,
			))
		} else {
			Ok(LaunchOutcome::Checked(self.check_updates(None).await?))
		}
	}
}
