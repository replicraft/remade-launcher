//! How installed apps are updated.

use egui::{ComboBox, RichText, Ui};
use launcher_background::control::BackgroundStatus;
use launcher_background::registration::RegistrationState;
use launcher_core::UpdateSettings;
use launcher_model::{MinimumAge, UpdateMode};

use super::SettingsAction;
use crate::ui::theme;

pub(super) fn updates(
	ui: &mut Ui,
	settings: &UpdateSettings,
	background: Option<&BackgroundStatus>,
	action: &mut SettingsAction,
) {
	// One of three, never three independent switches.
	for mode in UpdateMode::ALL {
		ui.horizontal_top(|ui| {
			if ui.radio(settings.mode == mode, "").clicked() && settings.mode != mode {
				*action = SettingsAction::PickMode(mode);
			}

			ui.vertical(|ui| {
				let title = RichText::new(mode.label()).family(theme::semibold());
				let title = if mode == UpdateMode::Daemon {
					title.color(ui.visuals().warn_fg_color)
				} else {
					title
				};

				ui.label(title);
				ui.label(
					RichText::new(mode.description())
						.small()
						.color(theme::muted(ui)),
				);
			});
		});

		ui.add_space(2.0);
	}

	ui.add_space(8.0);
	minimum_age(ui, settings, action);

	if settings.mode == UpdateMode::Daemon
		|| background.is_some_and(|status| status.running.is_some())
	{
		ui.add_space(8.0);
		background_status(ui, background, action);
	}
}

fn minimum_age(ui: &mut Ui, settings: &UpdateSettings, action: &mut SettingsAction) {
	ui.label(RichText::new("Auto-update Default Minimum Release Age").family(theme::semibold()));
	ui.label(
		RichText::new(if settings.mode.is_automatic() {
			"A release must be at least this old before an automatic update installs it. Each app can use its own."
		} else {
			"Used only by the automatic modes. It never holds back an update you start yourself."
		})
		.small()
		.color(theme::muted(ui)),
	);

	// Only the presets: a free-form duration is one more thing to get wrong.
	let current = MinimumAge::PRESETS
		.iter()
		.find(|(_, age)| *age == settings.minimum_age)
		.map_or_else(
			|| settings.minimum_age.short(),
			|(label, _)| (*label).to_owned(),
		);

	ComboBox::from_id_salt("minimum-age")
		.selected_text(current)
		.show_ui(ui, |ui| {
			for (label, age) in MinimumAge::PRESETS {
				if ui
					.selectable_label(settings.minimum_age == age, label)
					.clicked()
				{
					*action = SettingsAction::SetMinimumAge(age);
				}
			}
		});

	if settings.minimum_age == MinimumAge::NONE {
		ui.colored_label(
			ui.visuals().warn_fg_color,
			"No delay installs a release the moment it is published. That is riskier.",
		);
	}
}

/// The updater's real state. It is never "Active" just because the setting
/// is on.
fn background_status(
	ui: &mut Ui,
	background: Option<&BackgroundStatus>,
	action: &mut SettingsAction,
) {
	ui.label(RichText::new("Background updater").family(theme::semibold()));

	let Some(status) = background else {
		ui.label(RichText::new("Checking…").color(theme::muted(ui)));

		return;
	};

	let (label, detail) = match (&status.running, &status.registration) {
		(Some(_), _) => ("Active", None),
		(None, RegistrationState::Registered) => {
			("Stopped", Some("It will start when you sign in."))
		}
		(None, RegistrationState::PendingApproval) => (
			"Pending OS approval",
			Some("Approve it in your system's login items."),
		),
		(None, RegistrationState::DisabledByOs) => (
			"Disabled by OS",
			Some("You turned it off in system settings. The Launcher will not turn it back on."),
		),
		(None, RegistrationState::NotInstalled) => ("Not installed", None),
		(None, RegistrationState::Failed(why)) => ("Failed", Some(why.as_str())),
		(None, RegistrationState::Unsupported(why)) => ("Unavailable here", Some(why.as_str())),
	};

	ui.label(label);

	if let Some(detail) = detail {
		ui.label(RichText::new(detail).small().color(theme::muted(ui)));
	}

	if let Some(info) = &status.running {
		if let Some(at) = info.last_completed.or(info.last_attempted) {
			ui.label(
				RichText::new(format!(
					"Last check: {}",
					crate::ui::screens::sheet::when(at)
				))
				.small()
				.color(theme::muted(ui)),
			);
		}

		if let Some(outcome) = &info.last_outcome {
			ui.label(
				RichText::new(format!("Last result: {outcome}"))
					.small()
					.color(theme::muted(ui)),
			);
		}

		if let Some(next) = info.next_due {
			ui.label(
				RichText::new(format!(
					"Next check: {}",
					crate::ui::screens::sheet::when(next)
				))
				.small()
				.color(theme::muted(ui)),
			);
		}

		if ui.button("Stop background updates").clicked() {
			*action = SettingsAction::StopBackground;
		}
	}
}
