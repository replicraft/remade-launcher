//! Language, and how dates and times are written.

use egui::{RichText, Ui};
use launcher_model::local_time::{self, DateFormat, TimeFormat};
use launcher_model::{Language, Preferences};

use super::SettingsAction;
use crate::ui::components::{choice_row, setting_row};
use crate::ui::theme;

pub(super) fn region(ui: &mut Ui, prefs: &Preferences, action: &mut SettingsAction) {
	let system = local_time::system_locale();

	setting_row(
		ui,
		"Language",
		Some(&format!(
			"Automatic follows your system ({system}). English is the only language with text so far, so every choice shows English; the choice is remembered for when more arrive."
		)),
		|ui| {
			let mut chosen = prefs.language;
			let options: Vec<(Language, String)> = Language::ALL
				.iter()
				.map(|language| (*language, language.label().to_owned()))
				.collect();

			if choice_row(ui, "language", &mut chosen, &options) {
				let mut next = prefs.clone();

				next.language = chosen;
				*action = SettingsAction::Preferences(Box::new(next));
			}
		},
	);

	setting_row(
		ui,
		"Time format",
		Some("Automatic follows your system's region."),
		|ui| {
			let mut chosen = prefs.time_format;
			let options: Vec<(TimeFormat, String)> = TimeFormat::ALL
				.iter()
				.map(|format| (*format, format.label().to_owned()))
				.collect();

			if choice_row(ui, "time-format", &mut chosen, &options) {
				let mut next = prefs.clone();

				next.time_format = chosen;
				*action = SettingsAction::Preferences(Box::new(next));
			}
		},
	);

	setting_row(
		ui,
		"Date format",
		Some("Automatic follows your system's region."),
		|ui| {
			let mut chosen = prefs.date_format;
			let options: Vec<(DateFormat, String)> = DateFormat::ALL
				.iter()
				.map(|format| (*format, format.label().to_owned()))
				.collect();

			if choice_row(ui, "date-format", &mut chosen, &options) {
				let mut next = prefs.clone();

				next.date_format = chosen;
				*action = SettingsAction::Preferences(Box::new(next));
			}
		},
	);

	let now = time::OffsetDateTime::now_utc().unix_timestamp();

	ui.label(
		RichText::new(format!(
			"Example: {}   ·   {}",
			local_time::friendly(now, now),
			local_time::date_time(now - 3 * 86_400)
		))
		.small()
		.color(theme::muted(ui)),
	);
}
