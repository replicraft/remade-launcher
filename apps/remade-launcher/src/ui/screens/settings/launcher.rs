//! How the Launcher keeps itself up to date.
//!
//! Apart from how installed apps are updated: choosing one never changes the
//! other.

use egui::{RichText, Ui};
use launcher_core::LauncherUpdateReport;
use launcher_model::{LauncherUpdates, Preferences, local_time};

use super::SettingsAction;
use crate::ui::i18n::text_for;
use crate::ui::theme;

pub(super) fn launcher(
	ui: &mut Ui,
	prefs: &Preferences,
	report: Option<&LauncherUpdateReport>,
	checking: bool,
	action: &mut SettingsAction,
) {
	for mode in LauncherUpdates::ALL {
		ui.horizontal_top(|ui| {
			if ui.radio(prefs.launcher_updates == mode, "").clicked()
				&& prefs.launcher_updates != mode
			{
				let mut next = prefs.clone();

				next.launcher_updates = mode;
				*action = SettingsAction::Preferences(Box::new(next));
			}

			ui.vertical(|ui| {
				ui.label(
					RichText::new(text_for("launcher-updates", mode.as_str()))
						.family(theme::semibold()),
				);
				ui.label(
					RichText::new(t!(&format!(
						"launcher-updates-{}.description",
						mode.as_str()
					)))
					.small()
					.color(theme::muted(ui)),
				);
			});
		});

		ui.add_space(2.0);
	}

	ui.add_space(8.0);

	let Some(report) = report else {
		return;
	};

	let muted = theme::muted(ui);

	if let Some(at) = report.checked_at {
		ui.label(
			RichText::new(t!(
				"settings-launcher-last-checked",
				when = local_time::date_time(at)
			))
			.small()
			.color(muted),
		);
	}

	if let Some(staged) = &report.staged {
		ui.add_space(4.0);
		ui.label(
			RichText::new(t!(
				"settings-launcher-staged",
				version = staged.version.to_string()
			))
			.family(theme::semibold()),
		);
	} else if let Some(found) = &report.available {
		ui.add_space(4.0);
		ui.label(
			RichText::new(t!(
				"settings-launcher-available",
				version = found.version.to_string()
			))
			.family(theme::semibold()),
		);
	}

	if let Some(applied) = &report.applied {
		ui.label(
			RichText::new(t!(
				"settings-launcher-applied",
				from = applied.from.to_string(),
				to = applied.to.to_string(),
				when = local_time::date_time(applied.at)
			))
			.small()
			.color(muted),
		);
	}

	ui.add_space(8.0);

	ui.horizontal(|ui| {
		let can_download = report.blocker.is_none()
			&& report.staged.is_none()
			&& report.available.is_some()
			&& !checking;

		if ui
			.add_enabled(
				!checking,
				egui::Button::new(if checking {
					t!("settings-launcher-checking")
				} else {
					t!("settings-launcher-check-now")
				}),
			)
			.clicked()
		{
			*action = SettingsAction::CheckLauncherUpdate;
		}

		if can_download && ui.button(t!("settings-launcher-download")).clicked() {
			*action = SettingsAction::DownloadLauncherUpdate;
		}

		if report.can_roll_back && ui.button(t!("settings-launcher-roll-back")).clicked() {
			*action = SettingsAction::RollBackLauncher;
		}
	});
}
