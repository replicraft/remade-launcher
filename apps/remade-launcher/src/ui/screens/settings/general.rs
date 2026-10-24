//! Appearance and where the launcher starts.

use egui::{RichText, Ui};
use launcher_model::{Preferences, StartPage, ThemePreference};

use super::SettingsAction;
use crate::ui::components::{choice_row, setting_row};
use crate::ui::theme;

pub(super) fn general(
	ui: &mut Ui,
	theme_now: ThemePreference,
	prefs: &Preferences,
	action: &mut SettingsAction,
) {
	setting_row(ui, "Theme", None, |ui| {
		ui.horizontal(|ui| {
			for choice in ThemePreference::ALL {
				if ui.radio(theme_now == choice, choice.label()).clicked() {
					*action = SettingsAction::Theme(choice);
				}
			}
		});
	});

	setting_row(
		ui,
		"Interface size",
		Some("Makes everything larger or smaller. Useful on high-resolution screens."),
		|ui| {
			let mut chosen = prefs.ui_scale;
			let options: Vec<(u16, String)> = Preferences::SCALES
				.iter()
				.map(|scale| (*scale, format!("{scale}%")))
				.collect();

			if choice_row(ui, "ui-scale", &mut chosen, &options) {
				let mut next = prefs.clone();

				next.ui_scale = chosen;
				*action = SettingsAction::Preferences(Box::new(next));
			}
		},
	);

	setting_row(
		ui,
		"Open on",
		Some("The page the launcher shows when it starts."),
		|ui| {
			let mut chosen = prefs.start_page;
			let options: Vec<(StartPage, String)> = StartPage::ALL
				.iter()
				.map(|page| (*page, page.label().to_owned()))
				.collect();

			if choice_row(ui, "start-page", &mut chosen, &options) {
				let mut next = prefs.clone();

				next.start_page = chosen;
				*action = SettingsAction::Preferences(Box::new(next));
			}
		},
	);

	ui.label(RichText::new("Your layout, such as the folded sidebar, grid or list, and window size, is remembered automatically.").small().color(theme::muted(ui)));
}
