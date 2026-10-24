//! A plain outlined button with its label centred, for secondary actions.

use egui::{Align2, FontId, Response, Role, Sense, Stroke, Ui, Vec2, WidgetInfo};

use crate::ui::theme;

/// Painted by hand so the label is exactly in the middle at any width.
pub(crate) fn quiet_button(ui: &mut Ui, label: &str, enabled: bool, size: Vec2) -> Response {
	let palette = theme::palette(ui);
	let sense = if enabled {
		Sense::click()
	} else {
		Sense::hover()
	};
	let (rect, response) = ui.allocate_exact_size(size, sense);

	response.widget_info(|| WidgetInfo::labeled(Role::Button, enabled, label));

	if ui.is_rect_visible(rect) {
		let fill = if !enabled {
			palette.surface
		} else if response.is_pointer_button_down_on() {
			palette.surface_active
		} else if response.hovered() {
			palette.surface_hover
		} else {
			palette.surface
		};
		let ink = if enabled {
			palette.text
		} else {
			palette.text_muted.gamma_multiply(0.6)
		};
		let outline = if response.has_focus() {
			palette.accent
		} else {
			palette.border
		};

		ui.painter().rect_filled(rect, 10.0, fill);
		ui.painter().rect_stroke(
			rect,
			10.0,
			Stroke::new(1.0, outline),
			egui::StrokeKind::Inside,
		);
		ui.painter().text(
			rect.center(),
			Align2::CENTER_CENTER,
			label,
			FontId::proportional(15.0),
			ink,
		);
	}

	if enabled {
		response.on_hover_cursor(egui::CursorIcon::PointingHand)
	} else {
		response
	}
}
