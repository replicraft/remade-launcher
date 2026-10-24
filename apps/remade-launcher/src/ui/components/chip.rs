//! A pill-shaped choice, such as a category filter.

use egui::{Align2, FontId, Role, Sense, Ui, WidgetInfo, vec2};

use crate::ui::theme;

/// Returns true when clicked.
pub(crate) fn chip(ui: &mut Ui, text: &str, selected: bool) -> bool {
	let palette = theme::palette(ui);
	let font = FontId::proportional(13.5);
	let galley = ui
		.painter()
		.layout_no_wrap(text.to_owned(), font.clone(), palette.text);
	let size = vec2(galley.size().x + 26.0, 30.0);
	let (rect, response) = ui.allocate_exact_size(size, Sense::click());

	response.widget_info(|| WidgetInfo::selected(Role::RadioButton, true, selected, text));

	if ui.is_rect_visible(rect) {
		let fill = if selected {
			palette.accent.gamma_multiply(0.22)
		} else if response.hovered() {
			palette.surface_hover
		} else {
			palette.surface
		};
		let outline = if selected {
			palette.accent
		} else {
			palette.border
		};
		let color = if selected {
			palette.accent
		} else {
			palette.text
		};

		ui.painter().rect_filled(rect, 15.0, fill);
		ui.painter().rect_stroke(
			rect,
			15.0,
			egui::Stroke::new(1.0, outline),
			egui::StrokeKind::Inside,
		);
		ui.painter()
			.text(rect.center(), Align2::CENTER_CENTER, text, font, color);
	}

	response
		.on_hover_cursor(egui::CursorIcon::PointingHand)
		.clicked()
}
