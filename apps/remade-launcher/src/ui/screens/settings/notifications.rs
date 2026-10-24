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
			t!("settings-notify-updates"),
		)
		.changed();
	changed |= ui
		.checkbox(
			&mut next.notifications.install_finished,
			t!("settings-notify-finished"),
		)
		.changed();

	ui.label(
		RichText::new(t!("settings-notify-note"))
			.small()
			.color(theme::muted(ui)),
	);

	if changed {
		*action = SettingsAction::Preferences(Box::new(next));
	}

	ui.add_space(6.0);

	if ui.button(t!("settings-notify-test")).clicked() {
		*action = SettingsAction::TestNotification;
	}
}
