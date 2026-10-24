//! Appearance and where the launcher starts.

use egui::{RichText, Ui};
use launcher_model::{Preferences, StartPage, ThemePreference};

use super::SettingsAction;
use crate::ui::components::{choice_row, setting_row};
use crate::ui::i18n::text_for;
use crate::ui::theme;

pub(super) fn general(
	ui: &mut Ui,
	theme_now: ThemePreference,
	prefs: &Preferences,
	action: &mut SettingsAction,
) {
	setting_row(ui, &t!("settings-theme"), None, |ui| {
		ui.horizontal(|ui| {
			for choice in ThemePreference::ALL {
				if ui
					.radio(theme_now == choice, text_for("theme", choice.as_str()))
					.clicked()
				{
					*action = SettingsAction::Theme(choice);
				}
			}
		});
	});

	setting_row(
		ui,
		&t!("settings-scale"),
		Some(&t!("settings-scale.description")),
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
		&t!("settings-start-page"),
		Some(&t!("settings-start-page.description")),
		|ui| {
			let mut chosen = prefs.start_page;
			let options: Vec<(StartPage, String)> = StartPage::ALL
				.iter()
				.map(|page| (*page, text_for("start-page", page.as_str())))
				.collect();

			if choice_row(ui, "start-page", &mut chosen, &options) {
				let mut next = prefs.clone();

				next.start_page = chosen;
				*action = SettingsAction::Preferences(Box::new(next));
			}
		},
	);

	ui.label(
		RichText::new(t!("settings-layout-note"))
			.small()
			.color(theme::muted(ui)),
	);
}
