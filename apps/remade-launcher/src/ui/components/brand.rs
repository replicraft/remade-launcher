//! The logo, and beside it the name and tagline when there is room.

use egui::{RichText, TextureHandle, Ui, vec2};
use launcher_model::product;

use crate::ui::theme;

pub(crate) fn brand(ui: &mut Ui, logo: &TextureHandle, compact: bool) {
	ui.horizontal(|ui| {
		ui.image((logo.id(), vec2(34.0, 34.0)));

		if compact {
			return;
		}

		ui.vertical(|ui| {
			ui.spacing_mut().item_spacing.y = 0.0;
			ui.label(
				RichText::new(product::NAME)
					.size(16.0)
					.family(theme::semibold()),
			);
			ui.label(
				RichText::new(product::TAGLINE)
					.size(11.0)
					.color(theme::muted(ui)),
			);
		});
	});
}
