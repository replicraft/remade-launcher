//! Asking a running install or update to stop.
//!
//! Stopping is only offered at safe points: while the download is in flight
//! and between the steps that only touch staging. Once files start moving
//! into place the operation is committed and runs to the end, because a
//! half-moved install is what recovery exists to clean up, not something to
//! create on purpose.

use std::sync::{Arc, Mutex};

use launcher_model::{ErrorCode, LauncherError};
use tokio::sync::Notify;

#[derive(Default)]
struct State {
	active: bool,
	requested: bool,
	committed: bool,
}

#[derive(Clone, Default)]
pub(crate) struct CancelSignal {
	state: Arc<Mutex<State>>,
	notify: Arc<Notify>,
}

impl CancelSignal {
	fn state(&self) -> std::sync::MutexGuard<'_, State> {
		self.state
			.lock()
			.unwrap_or_else(|poisoned| poisoned.into_inner())
	}

	/// A new operation starts with a clean slate.
	pub(crate) fn begin(&self) {
		*self.state() = State {
			active: true,
			..State::default()
		};
	}

	pub(crate) fn finish(&self) {
		*self.state() = State::default();
	}

	/// Asks the running operation to stop. False means there is nothing to
	/// stop, or it is past the point where stopping is safe.
	pub(crate) fn request(&self) -> bool {
		let accepted = {
			let mut state = self.state();

			if !state.active || state.committed {
				false
			} else {
				state.requested = true;

				true
			}
		};

		if accepted {
			self.notify.notify_waiters();
		}

		accepted
	}

	/// Fails with [`ErrorCode::Cancelled`] if a stop was asked for.
	pub(crate) fn check(&self) -> Result<(), LauncherError> {
		if self.state().requested {
			Err(cancelled())
		} else {
			Ok(())
		}
	}

	/// Passes the last safe point. After this a stop request is refused.
	pub(crate) fn commit(&self) -> Result<(), LauncherError> {
		let mut state = self.state();

		if state.requested {
			return Err(cancelled());
		}

		state.committed = true;

		Ok(())
	}

	/// Completes when a stop is requested.
	pub(crate) async fn requested(&self) {
		loop {
			let waiting = self.notify.notified();

			tokio::pin!(waiting);
			waiting.as_mut().enable();

			if self.state().requested {
				return;
			}

			waiting.await;
		}
	}
}

fn cancelled() -> LauncherError {
	LauncherError::new(ErrorCode::Cancelled, "Stopped. Nothing was installed.")
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn nothing_to_stop_when_idle() {
		assert!(!CancelSignal::default().request());
	}

	#[test]
	fn a_request_before_the_commit_point_stops_the_operation() {
		let signal = CancelSignal::default();

		signal.begin();
		assert!(signal.request());
		assert_eq!(signal.check().unwrap_err().code(), ErrorCode::Cancelled);
		assert_eq!(signal.commit().unwrap_err().code(), ErrorCode::Cancelled);
	}

	#[test]
	fn after_the_commit_point_a_request_is_refused() {
		let signal = CancelSignal::default();

		signal.begin();
		signal.commit().unwrap();

		assert!(!signal.request());
		assert!(signal.check().is_ok());
	}

	#[test]
	fn each_operation_starts_clean() {
		let signal = CancelSignal::default();

		signal.begin();
		signal.request();
		signal.finish();
		signal.begin();

		assert!(signal.check().is_ok());
	}

	#[tokio::test]
	async fn waiting_wakes_when_a_stop_is_requested() {
		let signal = CancelSignal::default();

		signal.begin();

		let waiter = signal.clone();
		let task = tokio::spawn(async move { waiter.requested().await });

		tokio::task::yield_now().await;
		signal.request();

		tokio::time::timeout(std::time::Duration::from_secs(2), task)
			.await
			.expect("the waiter woke")
			.unwrap();
	}
}
