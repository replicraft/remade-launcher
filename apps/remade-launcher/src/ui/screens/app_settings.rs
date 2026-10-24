//! One app's own update preferences, in a dialog. They can pause or delay an
//! update, never authorize one.

use egui::{ComboBox, RichText, Ui};
use launcher_core::InstalledApp;
use launcher_model::MinimumAge;
use launcher_storage::AppPolicy;

use super::Action;
use crate::ui::theme;

/// Draws the controls. Returns the change the person just made, if any.
pub(crate) fn app_settings(
	ui: &mut Ui,
	app: &InstalledApp,
	policy: &AppPolicy,
	global_age: MinimumAge,
) -> Option<Action> {
	let mut action = None;
	let mut changed = policy.clone();
	let mut pinned = policy.pinned_release_id.is_some();

	if ui.checkbox(&mut pinned, "Pin at this version").changed() {
		action = Some(Action::Pin(app.id, pinned));
	}

	ui.label(
		RichText::new(
			"A pinned app is never updated, by hand or automatically, until you unpin it.",
		)
		.small()
		.color(theme::muted(ui)),
	);
	ui.add_space(6.0);

	ui.checkbox(
		&mut changed.auto_paused,
		"Pause automatic updates for this app",
	);
	ui.checkbox(&mut changed.allow_prerelease, "Allow pre-releases");
	ui.add_space(6.0);

	ui.horizontal(|ui| {
		ui.label("Minimum release age");

		let effective = match changed.min_age_seconds.and_then(MinimumAge::from_seconds) {
			Some(age) => preset_label(age),
			None => format!("Use global default ({})", preset_label(global_age)),
		};

		ComboBox::from_id_salt(("age", app.id.to_string()))
			.selected_text(effective)
			.show_ui(ui, |ui| {
				if ui
					.selectable_label(changed.min_age_seconds.is_none(), "Use global default")
					.clicked()
				{
					changed.min_age_seconds = None;
				}

				for (label, age) in MinimumAge::PRESETS {
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
		.map_or_else(|| age.short(), |(label, _)| (*label).to_owned())
}
