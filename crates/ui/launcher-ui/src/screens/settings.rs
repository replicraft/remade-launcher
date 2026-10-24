use egui::{RichText, Ui};
use launcher_model::{ThemePreference, product};

use super::page;
use crate::theme;

pub(crate) enum SettingsAction {
	None,
	Theme(ThemePreference),
	CopyDiagnostics,
}

pub(crate) fn settings(ui: &mut Ui, current: ThemePreference) -> SettingsAction {
	let mut action = SettingsAction::None;

	page(ui, "Settings", |ui| {
		ui.label(RichText::new("Appearance").size(18.0).strong());

		ui.horizontal(|ui| {
			for theme in ThemePreference::ALL {
				if ui.radio(current == theme, theme.label()).clicked() {
					action = SettingsAction::Theme(theme);
				}
			}
		});

		ui.add_space(16.0);
		ui.label(RichText::new("About").size(18.0).strong());
		ui.label(format!("{} {}", product::NAME, product::VERSION));
		ui.label(RichText::new(product::TAGLINE).color(theme::muted(ui)));

		ui.add_space(8.0);

		if ui.button("Copy support report").clicked() {
			action = SettingsAction::CopyDiagnostics;
		}

		ui.label(
			RichText::new(
				"The report lists versions and counts only. It has no file paths, links, or app names.",
			)
			.small()
			.color(theme::muted(ui)),
		);
	});

	action
}
