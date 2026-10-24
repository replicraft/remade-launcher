use egui::{Frame, Margin, RichText, TextureHandle, Ui};
use launcher_model::product;

use super::Screen;
use crate::theme;

pub(crate) fn navigation(ui: &mut Ui, logo: &TextureHandle, current: &mut Screen) {
	Frame::new().inner_margin(Margin::same(16)).show(ui, |ui| {
		ui.horizontal(|ui| {
			ui.image((logo.id(), egui::vec2(36.0, 36.0)));
			ui.label(RichText::new("Remade").size(18.0).strong());
		});

		ui.add_space(20.0);

		for screen in Screen::ALL {
			let entry = ui.add_sized(
				[ui.available_width(), 36.0],
				egui::Button::selectable(*current == screen, screen.title()),
			);

			if entry.clicked() {
				*current = screen;
			}
		}

		ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
			ui.label(
				RichText::new(format!("v{}", product::VERSION))
					.small()
					.color(theme::muted(ui)),
			);
		});
	});
}
