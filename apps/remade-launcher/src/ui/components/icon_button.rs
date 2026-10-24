//! A square button that shows only an icon.
//!
//! The label is never drawn: it is the accessible name and the hover text.

use egui::{Align2, FontId, Role, Sense, Ui, WidgetInfo, vec2};

use crate::ui::theme;

pub(crate) fn icon_button(ui: &mut Ui, icon: &str, label: &str, size: f32) -> egui::Response {
	let palette = theme::palette(ui);
	let (rect, response) = ui.allocate_exact_size(vec2(size, size), Sense::click());

	response.widget_info(|| WidgetInfo::labeled(Role::Button, true, label));

	if ui.is_rect_visible(rect) {
		let painter = ui.painter();

		if response.hovered() {
			painter.rect_filled(rect, size / 4.0, palette.surface_hover);
		}

		if response.has_focus() {
			painter.rect_stroke(
				rect,
				size / 4.0,
				egui::Stroke::new(1.5, palette.accent),
				egui::StrokeKind::Inside,
			);
		}

		let color = if response.hovered() {
			palette.text
		} else {
			palette.text_muted
		};

		painter.text(
			rect.center(),
			Align2::CENTER_CENTER,
			icon,
			FontId::proportional(size * 0.62),
			color,
		);
	}

	response
		.on_hover_text(label)
		.on_hover_cursor(egui::CursorIcon::PointingHand)
}
