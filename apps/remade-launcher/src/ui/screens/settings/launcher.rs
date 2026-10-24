//! How the Launcher keeps itself up to date.
//!
//! Apart from how installed apps are updated: choosing one never changes the
//! other.

use egui::{RichText, Ui};
use launcher_core::LauncherUpdateReport;
use launcher_model::{LauncherUpdates, Preferences, local_time};

use super::SettingsAction;
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
				ui.label(RichText::new(mode.label()).family(theme::semibold()));
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

	let Some(report) = report else {
		return;
	};

	let muted = theme::muted(ui);

	if let Some(at) = report.checked_at {
		ui.label(
			RichText::new(format!("Last checked {}", local_time::date_time(at)))
				.small()
				.color(muted),
		);
	}

	if let Some(staged) = &report.staged {
		ui.add_space(4.0);
		ui.label(
			RichText::new(format!(
				"{} is downloaded. It starts the next time you open the launcher.",
				staged.version
			))
			.family(theme::semibold()),
		);
	} else if let Some(found) = &report.available {
		ui.add_space(4.0);
		ui.label(
			RichText::new(format!("{} is available.", found.version)).family(theme::semibold()),
		);
	}

	if let Some(applied) = &report.applied {
		ui.label(
			RichText::new(format!(
				"Switched from {} to {} on {}.",
				applied.from,
				applied.to,
				local_time::date_time(applied.at)
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
				egui::Button::new(if checking { "Checking…" } else { "Check now" }),
			)
			.clicked()
		{
			*action = SettingsAction::CheckLauncherUpdate;
		}

		if can_download && ui.button("Download now").clicked() {
			*action = SettingsAction::DownloadLauncherUpdate;
		}

		if report.can_roll_back && ui.button("Go back to the previous version").clicked() {
			*action = SettingsAction::RollBackLauncher;
		}
	});
}
