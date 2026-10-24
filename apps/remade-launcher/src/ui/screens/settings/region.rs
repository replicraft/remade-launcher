//! Language, and how dates and times are written.

use egui::{RichText, Ui};
use launcher_model::local_time::{self, DateFormat, TimeFormat};
use launcher_model::{Language, Preferences};

use super::SettingsAction;
use crate::ui::components::{choice_row, setting_row};
use crate::ui::i18n::text_for;
use crate::ui::theme;

pub(super) fn region(ui: &mut Ui, prefs: &Preferences, action: &mut SettingsAction) {
	let system = local_time::system_locale();

	setting_row(
		ui,
		&t!("settings-language"),
		Some(&t!("settings-language.description", system = system)),
		|ui| {
			let mut chosen = prefs.language;
			// Each language is named in itself, so it can be found whatever the
			// interface says now.
			let options: Vec<(Language, String)> = Language::ALL
				.iter()
				.map(|language| {
					let name = language
						.endonym()
						.map_or_else(|| t!("settings-language-auto"), str::to_owned);

					(*language, name)
				})
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
		&t!("settings-time-format"),
		Some(&t!("settings-time-format.description")),
		|ui| {
			let mut chosen = prefs.time_format;
			let options: Vec<(TimeFormat, String)> = TimeFormat::ALL
				.iter()
				.map(|format| (*format, text_for("time-format", format.as_str())))
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
		&t!("settings-date-format"),
		Some(&t!("settings-date-format.description")),
		|ui| {
			let mut chosen = prefs.date_format;
			let options: Vec<(DateFormat, String)> = DateFormat::ALL
				.iter()
				.map(|format| (*format, text_for("date-format", format.as_str())))
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
		RichText::new(t!(
			"settings-region-example",
			today = local_time::friendly(now, now),
			earlier = local_time::date_time(now - 3 * 86_400)
		))
		.small()
		.color(theme::muted(ui)),
	);
}
