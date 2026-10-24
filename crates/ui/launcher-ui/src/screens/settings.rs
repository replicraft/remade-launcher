use egui::{RichText, Ui};
use launcher_model::ThemePreference;

use super::page;

/// Returns the theme the user just picked, if they picked one.
pub(crate) fn settings(
	ui: &mut Ui,
	current: ThemePreference,
	notice: Option<&str>,
) -> Option<ThemePreference> {
	let mut chosen = None;

	page(ui, "Settings", |ui| {
		ui.label(RichText::new("Appearance").size(18.0).strong());

		ui.horizontal(|ui| {
			for theme in ThemePreference::ALL {
				if ui.radio(current == theme, theme.label()).clicked() {
					chosen = Some(theme);
				}
			}
		});

		if let Some(notice) = notice {
			ui.add_space(12.0);
			ui.colored_label(ui.visuals().error_fg_color, notice);
		}
	});

	chosen
}
