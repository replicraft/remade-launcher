//! How installed apps are updated.

use egui::{ComboBox, RichText, Ui};
use launcher_background::control::BackgroundStatus;
use launcher_background::registration::RegistrationState;
use launcher_core::UpdateSettings;
use launcher_model::{MinimumAge, UpdateMode};

use super::SettingsAction;
use crate::ui::i18n::text_for;
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
				let title =
					RichText::new(text_for("update-mode", mode.as_str())).family(theme::semibold());
				let title = if mode == UpdateMode::Daemon {
					title.color(ui.visuals().warn_fg_color)
				} else {
					title
				};

				ui.label(title);
				ui.label(
					RichText::new(t!(&format!("update-mode-{}.description", mode.as_str())))
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
	ui.label(RichText::new(t!("settings-min-age")).family(theme::semibold()));
	ui.label(
		RichText::new(if settings.mode.is_automatic() {
			t!("settings-min-age.automatic")
		} else {
			t!("settings-min-age.manual")
		})
		.small()
		.color(theme::muted(ui)),
	);

	// Only the presets: a free-form duration is one more thing to get wrong.
	let current = MinimumAge::PRESETS
		.iter()
		.find(|(_, age)| *age == settings.minimum_age)
		.map_or_else(|| settings.minimum_age.short(), |(_, age)| age_label(*age));

	ComboBox::from_id_salt("minimum-age")
		.selected_text(current)
		.show_ui(ui, |ui| {
			for (_, age) in MinimumAge::PRESETS {
				if ui
					.selectable_label(settings.minimum_age == age, age_label(age))
					.clicked()
				{
					*action = SettingsAction::SetMinimumAge(age);
				}
			}
		});

	if settings.minimum_age == MinimumAge::NONE {
		ui.colored_label(ui.visuals().warn_fg_color, t!("settings-min-age-warning"));
	}
}

/// A preset delay in words, such as "6 hours".
fn age_label(age: MinimumAge) -> String {
	t!(&format!("minimum-age-{}", age.seconds()))
}

/// The updater's real state. It is never "Active" just because the setting
/// is on.
fn background_status(
	ui: &mut Ui,
	background: Option<&BackgroundStatus>,
	action: &mut SettingsAction,
) {
	ui.label(RichText::new(t!("settings-background")).family(theme::semibold()));

	let Some(status) = background else {
		ui.label(RichText::new(t!("settings-background-checking")).color(theme::muted(ui)));

		return;
	};

	let (label, detail) = match (&status.running, &status.registration) {
		(Some(_), _) => (t!("settings-background-active"), None),
		(None, RegistrationState::Registered) => (
			t!("settings-background-stopped"),
			Some(t!("settings-background-stopped.detail")),
		),
		(None, RegistrationState::PendingApproval) => (
			t!("settings-background-pending"),
			Some(t!("settings-background-pending.detail")),
		),
		(None, RegistrationState::DisabledByOs) => (
			t!("settings-background-disabled"),
			Some(t!("settings-background-disabled.detail")),
		),
		(None, RegistrationState::NotInstalled) => (t!("settings-background-not-installed"), None),
		// The reasons come from the system and stay as they are.
		(None, RegistrationState::Failed(why)) => {
			(t!("settings-background-failed"), Some(why.clone()))
		}
		(None, RegistrationState::Unsupported(why)) => {
			(t!("settings-background-unavailable"), Some(why.clone()))
		}
	};

	ui.label(label);

	if let Some(detail) = detail {
		ui.label(RichText::new(detail).small().color(theme::muted(ui)));
	}

	if let Some(info) = &status.running {
		if let Some(at) = info.last_completed.or(info.last_attempted) {
			ui.label(
				RichText::new(t!(
					"settings-background-last-check",
					when = crate::ui::screens::sheet::when(at)
				))
				.small()
				.color(theme::muted(ui)),
			);
		}

		if let Some(outcome) = &info.last_outcome {
			ui.label(
				RichText::new(t!(
					"settings-background-last-result",
					outcome = outcome.as_str()
				))
				.small()
				.color(theme::muted(ui)),
			);
		}

		if let Some(next) = info.next_due {
			ui.label(
				RichText::new(t!(
					"settings-background-next-check",
					when = crate::ui::screens::sheet::when(next)
				))
				.small()
				.color(theme::muted(ui)),
			);
		}

		if ui.button(t!("settings-background-stop")).clicked() {
			*action = SettingsAction::StopBackground;
		}
	}
}
