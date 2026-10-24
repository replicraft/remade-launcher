//! Desktop notifications.
//!
//! Sent only when the person has turned them on, and only while the window is
//! in the background, since a notification about something already on screen
//! is noise. The system may ask permission the first time.

use launcher_model::product;

/// Shows a notification without waiting for it, since some platforms block
/// until it is dismissed. A failure is logged and otherwise ignored: a
/// notification that cannot be shown is not worth interrupting anything for.
pub(crate) fn send(title: &str, body: &str) {
	let (title, body) = (title.to_owned(), body.to_owned());

	std::thread::spawn(move || {
		let result = notify_rust::Notification::new()
			.appname(product::NAME)
			.summary(&title)
			.body(&body)
			.show();

		if let Err(error) = result {
			tracing::debug!(%error, "a notification could not be shown");
		}
	});
}

/// The wording for finished installs, in one place so it stays consistent.
pub(crate) fn install_finished(name: &str) -> (String, String) {
	(
		format!("{name} is installed"),
		"Open the launcher to start it.".to_owned(),
	)
}

pub(crate) fn install_failed(name: &str) -> (String, String) {
	(
		format!("{name} could not be installed"),
		"Open the launcher to see why.".to_owned(),
	)
}

pub(crate) fn updates_available(count: usize) -> (String, String) {
	let title = if count == 1 {
		"1 update is available".to_owned()
	} else {
		format!("{count} updates are available")
	};

	(
		title,
		"Open the launcher to review and install them.".to_owned(),
	)
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn the_wording_handles_one_and_many() {
		assert_eq!(updates_available(1).0, "1 update is available");
		assert_eq!(updates_available(3).0, "3 updates are available");
		assert_eq!(install_finished("Quill").0, "Quill is installed");
		assert_eq!(install_failed("Quill").0, "Quill could not be installed");
	}
}
