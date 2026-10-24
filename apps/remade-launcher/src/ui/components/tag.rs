//! A small, non-interactive label pill, such as a category or license.

use egui::{Align2, FontId, Sense, Ui, vec2};

use crate::ui::theme;

pub(crate) fn tag(ui: &mut Ui, text: &str) {
	let palette = theme::palette(ui);
	let font = FontId::proportional(12.5);
	let galley = ui
		.painter()
		.layout_no_wrap(text.to_owned(), font.clone(), palette.text_muted);
	let (rect, _) = ui.allocate_exact_size(vec2(galley.size().x + 20.0, 24.0), Sense::hover());

	ui.painter().rect_filled(rect, 12.0, palette.surface_hover);
	ui.painter().text(
		rect.center(),
		Align2::CENTER_CENTER,
		text,
		font,
		palette.text_muted,
	);
}
