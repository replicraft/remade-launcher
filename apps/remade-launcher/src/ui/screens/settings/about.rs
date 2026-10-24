//! What this program is.

use egui::{RichText, Ui};
use launcher_model::product;

use super::SettingsAction;
use crate::ui::components::danger_button;
use crate::ui::{icons, theme};

pub(super) fn about(ui: &mut Ui, logo: &egui::TextureHandle, action: &mut SettingsAction) {
	ui.horizontal(|ui| {
		ui.image((logo.id(), egui::vec2(56.0, 56.0)));

		ui.vertical(|ui| {
			ui.spacing_mut().item_spacing.y = 2.0;
			ui.label(
				RichText::new(format!("{} by Replicraft", product::NAME))
					.size(16.0)
					.family(theme::semibold()),
			);
			ui.label(
				RichText::new(format!("Version {}", product::VERSION)).color(theme::muted(ui)),
			);
			ui.label(
				RichText::new(product::TAGLINE)
					.small()
					.color(theme::muted(ui)),
			);
		});
	});

	ui.add_space(10.0);

	egui::Grid::new("about-facts")
		.num_columns(2)
		.spacing([16.0, 6.0])
		.show(ui, |ui| {
			fact(
				ui,
				"Platform",
				&format!("{} {}", std::env::consts::OS, std::env::consts::ARCH),
			);
			fact(ui, "License", env!("CARGO_PKG_LICENSE"));

			ui.label(RichText::new("Website").color(theme::muted(ui)));
			ui.hyperlink_to("remade.wiki/launcher", env!("CARGO_PKG_HOMEPAGE"));
			ui.end_row();
		});

	ui.add_space(8.0);

	ui.hyperlink_to(
		format!("{}  View the source on GitHub", icons::GITHUB),
		env!("CARGO_PKG_REPOSITORY"),
	);

	ui.add_space(12.0);

	// The consequences are spelled out in the confirmation that follows.
	if danger_button(ui, "Restore defaults").clicked() {
		*action = SettingsAction::RestoreDefaults;
	}
}

fn fact(ui: &mut Ui, label: &str, value: &str) {
	ui.label(RichText::new(label).color(theme::muted(ui)));
	ui.label(value);
	ui.end_row();
}
