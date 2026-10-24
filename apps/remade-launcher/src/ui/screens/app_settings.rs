//! One app's own update preferences, in a dialog. They can pause or delay an
//! update, never authorize one.

use egui::{ComboBox, RichText, Ui};
use launcher_core::{InstalledApp, UpdateSettings, effective_mode};
use launcher_model::{MinimumAge, UpdateMode};
use launcher_storage::AppPolicy;

use super::Action;
use crate::ui::components::{level_selector, quiet_button};
use crate::ui::i18n::text_for;
use crate::ui::screens::settings::updates::update_levels;
use crate::ui::theme;

/// Draws the controls. Returns the change the person just made, if any.
pub(crate) fn app_settings(
	ui: &mut Ui,
	app: &InstalledApp,
	policy: &AppPolicy,
	global: UpdateSettings,
) -> Option<Action> {
	let mut action = None;
	let mut changed = policy.clone();
	let global_age = global.minimum_age;
	let level = effective_mode(policy, global.mode);
	let own_choice = policy.pinned_release_id.is_some() || policy.update_mode.is_some();

	ui.label(RichText::new(t!("appsettings-level")).family(theme::semibold()));
	ui.add_space(4.0);

	// The always-on updater exists only while all apps are set to use it.
	let levels = update_levels(
		global.mode == UpdateMode::Daemon,
		Some(t!("appsettings-daemon-needs")),
	);

	if let Some(picked) = level_selector(ui, ("level", app.id.to_string()), level, &levels) {
		action = Some(Action::SetLevel(app.id, Some(picked)));
	}

	ui.add_space(2.0);

	if own_choice {
		if quiet_button(ui, &t!("appsettings-follow"), true, egui::vec2(280.0, 30.0)).clicked() {
			action = Some(Action::SetLevel(app.id, None));
		}
	} else {
		ui.label(
			RichText::new(t!(
				"appsettings-following",
				level = text_for("update-mode", global.mode.as_str())
			))
			.small()
			.color(theme::muted(ui)),
		);
	}

	ui.add_space(10.0);

	ui.checkbox(&mut changed.allow_prerelease, t!("appsettings-prerelease"));
	ui.add_space(6.0);

	ui.horizontal(|ui| {
		ui.label(t!("appsettings-min-age"));

		let effective = match changed.min_age_seconds.and_then(MinimumAge::from_seconds) {
			Some(age) => preset_label(age),
			None => t!(
				"appsettings-age-global-current",
				age = preset_label(global_age)
			),
		};

		ComboBox::from_id_salt(("age", app.id.to_string()))
			.selected_text(effective)
			.show_ui(ui, |ui| {
				if ui
					.selectable_label(
						changed.min_age_seconds.is_none(),
						t!("appsettings-age-global"),
					)
					.clicked()
				{
					changed.min_age_seconds = None;
				}

				for (_, age) in MinimumAge::PRESETS {
					let label = preset_label(age);

					if ui
						.selectable_label(changed.min_age_seconds == Some(age.seconds()), label)
						.clicked()
					{
						changed.min_age_seconds = Some(age.seconds());
					}
				}
			});
	});

	if changed != *policy {
		action = Some(Action::SetPolicy(app.id, changed));
	}

	action
}

fn preset_label(age: MinimumAge) -> String {
	MinimumAge::PRESETS
		.iter()
		.find(|(_, preset)| *preset == age)
		.map_or_else(
			|| age.short(),
			|(_, preset)| text_for("appsettings-age", &preset.seconds().to_string()),
		)
}
