//! The big call-to-action button, such as Install.

use egui::{Align2, Color32, FontId, Response, Role, Sense, Stroke, Ui, Vec2, WidgetInfo};

use crate::ui::theme;

/// Filled with the accent colour, lighter under the pointer and darker while
/// pressed. A disabled one is dimmed and explains itself elsewhere.
///
/// It is painted by hand so the label sits exactly in the middle, whatever
/// the surrounding layout does to a normal button's width.
pub(crate) fn primary_button(ui: &mut Ui, label: &str, enabled: bool, size: Vec2) -> Response {
	let palette = theme::palette(ui);
	let sense = if enabled {
		Sense::click()
	} else {
		Sense::hover()
	};
	let (rect, response) = ui.allocate_exact_size(size, sense);

	response.widget_info(|| WidgetInfo::labeled(Role::Button, enabled, label));

	if ui.is_rect_visible(rect) {
		let base = palette.accent;
		let fill = if !enabled {
			base.gamma_multiply(0.35)
		} else if response.is_pointer_button_down_on() {
			base.lerp_to_gamma(Color32::BLACK, 0.18)
		} else if response.hovered() {
			base.lerp_to_gamma(Color32::WHITE, 0.14)
		} else {
			base
		};
		// Dark text on the light blue of the dark theme, white on the deeper
		// blue of the light theme, so the label always reads.
		let ink = if ui.visuals().dark_mode {
			Color32::from_rgb(0x0B, 0x0B, 0x0C)
		} else {
			Color32::WHITE
		};
		let ink = if enabled {
			ink
		} else {
			ink.gamma_multiply(0.6)
		};

		ui.painter().rect_filled(rect, 14.0, fill);

		if response.has_focus() {
			ui.painter().rect_stroke(
				rect,
				14.0,
				Stroke::new(2.0, palette.text),
				egui::StrokeKind::Outside,
			);
		}

		ui.painter().text(
			rect.center(),
			Align2::CENTER_CENTER,
			label,
			FontId::new(21.0, theme::semibold()),
			ink,
		);
	}

	if enabled {
		response.on_hover_cursor(egui::CursorIcon::PointingHand)
	} else {
		response
	}
}
