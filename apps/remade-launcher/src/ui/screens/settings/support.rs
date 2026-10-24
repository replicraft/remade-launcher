//! Help for when something goes wrong.

use egui::{RichText, Ui};

use super::SettingsAction;
use crate::ui::theme;

pub(super) fn support(ui: &mut Ui, action: &mut SettingsAction) {
	if ui.button(t!("settings-support-copy")).clicked() {
		*action = SettingsAction::CopyDiagnostics;
	}

	ui.label(
		RichText::new(t!("settings-support-note"))
			.small()
			.color(theme::muted(ui)),
	);
}
