//! Which notifications to send.

use egui::{RichText, Ui};
use launcher_model::Preferences;

use super::SettingsAction;
use crate::ui::theme;

pub(super) fn notifications(ui: &mut Ui, prefs: &Preferences, action: &mut SettingsAction) {
	let mut next = prefs.clone();
	let mut changed = false;

	changed |= ui
		.checkbox(
			&mut next.notifications.updates_available,
			"Tell me when updates are available",
		)
		.changed();
	changed |= ui
		.checkbox(
			&mut next.notifications.install_finished,
			"Tell me when an install or update finishes",
		)
		.changed();

	ui.label(
		RichText::new("Notifications appear only while the launcher is in the background. Your system may ask permission the first time.")
			.small()
			.color(theme::muted(ui)),
	);

	if changed {
		*action = SettingsAction::Preferences(Box::new(next));
	}

	ui.add_space(6.0);

	if ui.button("Send a test notification").clicked() {
		*action = SettingsAction::TestNotification;
	}
}
