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
			ui.label(t!("settings-measuring"));
		});

		return;
	};

	setting_row(
		ui,
		&t!("settings-storage-apps"),
		Some(&t!("settings-storage-apps.description")),
		|ui| {
			ui.label(RichText::new(bytes(report.apps_bytes)).family(theme::semibold()));
			ui.label(
				RichText::new(report.apps_folder.display().to_string())
					.small()
					.monospace()
					.color(theme::muted(ui)),
			);

			if ui.button(t!("settings-show-folder")).clicked() {
				*action = SettingsAction::ShowFolder(report.apps_folder.clone());
			}
		},
	);

	setting_row(
		ui,
		&t!("settings-storage-leftover"),
		Some(&t!("settings-storage-leftover.description")),
		|ui| {
			ui.label(RichText::new(bytes(report.leftover_bytes)).family(theme::semibold()));

			if ui
				.add_enabled(
					report.leftover_bytes > 0,
					egui::Button::new(t!("settings-clear-leftovers")),
				)
				.clicked()
			{
				*action = SettingsAction::ClearLeftovers;
			}
		},
	);

	setting_row(
		ui,
		&t!("settings-storage-cache"),
		Some(&t!("settings-storage-cache.description")),
		|ui| {
			ui.label(RichText::new(bytes(report.cache_bytes)).family(theme::semibold()));

			if ui
				.add_enabled(
					report.cache_bytes > 0,
					egui::Button::new(t!("settings-clear-cache")),
				)
				.clicked()
			{
				*action = SettingsAction::ClearCache;
			}
		},
	);

	setting_row(
		ui,
		&t!("settings-storage-data"),
		Some(&t!("settings-storage-data.description")),
		|ui| {
			ui.label(RichText::new(bytes(report.database_bytes)).family(theme::semibold()));
			ui.label(
				RichText::new(report.data_folder.display().to_string())
					.small()
					.monospace()
					.color(theme::muted(ui)),
			);

			if ui.button(t!("settings-show-folder")).clicked() {
				*action = SettingsAction::ShowFolder(report.data_folder.clone());
			}
		},
	);
}
