//! Where things are kept, and clearing what is safe to clear.

use egui::{RichText, Ui};
use launcher_core::StorageReport;

use super::SettingsAction;
use crate::ui::components::setting_row;
use crate::ui::screens::sheet::bytes;
use crate::ui::theme;

pub(super) fn storage(ui: &mut Ui, report: Option<&StorageReport>, action: &mut SettingsAction) {
	let Some(report) = report else {
		ui.horizontal(|ui| {
			ui.spinner();
			ui.label("Measuring…");
		});

		return;
	};

	setting_row(
		ui,
		"Your apps",
		Some("Everything the launcher has installed. Uninstall an app to free its space."),
		|ui| {
			ui.label(RichText::new(bytes(report.apps_bytes)).family(theme::semibold()));
			ui.label(
				RichText::new(report.apps_folder.display().to_string())
					.small()
					.monospace()
					.color(theme::muted(ui)),
			);

			if ui.button("Show in file manager").clicked() {
				*action = SettingsAction::ShowFolder(report.apps_folder.clone());
			}
		},
	);

	setting_row(
		ui,
		"Leftover downloads",
		Some(
			"Files from installs that were interrupted. Safe to remove; nothing running is touched.",
		),
		|ui| {
			ui.label(RichText::new(bytes(report.leftover_bytes)).family(theme::semibold()));

			if ui
				.add_enabled(
					report.leftover_bytes > 0,
					egui::Button::new("Clear leftovers"),
				)
				.clicked()
			{
				*action = SettingsAction::ClearLeftovers;
			}
		},
	);

	setting_row(
		ui,
		"Remembered lookups",
		Some(
			"Answers from GitHub kept to make checks faster and cheaper. They are refilled as needed.",
		),
		|ui| {
			ui.label(RichText::new(bytes(report.cache_bytes)).family(theme::semibold()));

			if ui
				.add_enabled(
					report.cache_bytes > 0,
					egui::Button::new("Clear remembered lookups"),
				)
				.clicked()
			{
				*action = SettingsAction::ClearCache;
			}
		},
	);

	setting_row(
		ui,
		"Launcher data",
		Some("The launcher's own records: what is installed, settings, and history."),
		|ui| {
			ui.label(RichText::new(bytes(report.database_bytes)).family(theme::semibold()));
			ui.label(
				RichText::new(report.data_folder.display().to_string())
					.small()
					.monospace()
					.color(theme::muted(ui)),
			);

			if ui.button("Show in file manager").clicked() {
				*action = SettingsAction::ShowFolder(report.data_folder.clone());
			}
		},
	);
}
