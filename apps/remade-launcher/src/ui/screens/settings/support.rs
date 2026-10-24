//! Help for when something goes wrong.

use egui::{RichText, Ui};

use super::SettingsAction;
use crate::ui::theme;

pub(super) fn support(ui: &mut Ui, action: &mut SettingsAction) {
	if ui.button("Copy support report").clicked() {
		*action = SettingsAction::CopyDiagnostics;
	}

	ui.label(
		RichText::new(
			"The report lists versions and counts only. It has no file paths, links, or app names.",
		)
		.small()
		.color(theme::muted(ui)),
	);
}
