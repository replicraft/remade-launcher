//! A plain outlined button with its label centred, for secondary actions.

use egui::{Color32, FontId, Response, Role, Sense, Stroke, Ui, Vec2, WidgetInfo};

use crate::ui::theme;

/// Painted by hand so the label is exactly in the middle at any width.
pub(crate) fn quiet_button(ui: &mut Ui, label: &str, enabled: bool, size: Vec2) -> Response {
	paint(ui, None, label, enabled, size)
}

/// Like [`quiet_button`], with an icon in its own colour before the label.
pub(crate) fn quiet_button_with_icon(
	ui: &mut Ui,
	icon: (&str, Color32),
	label: &str,
	enabled: bool,
	size: Vec2,
) -> Response {
	paint(ui, Some(icon), label, enabled, size)
}

fn paint(
	ui: &mut Ui,
	icon: Option<(&str, Color32)>,
	label: &str,
	enabled: bool,
	size: Vec2,
) -> Response {
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
		let font = FontId::proportional(15.0);
		let text = ui
			.painter()
			.layout_no_wrap(label.to_owned(), font.clone(), ink);

		match icon {
			Some((glyph, color)) => {
				// The icon keeps its own colour; the label keeps the ink.
				let color = if enabled {
					color
				} else {
					color.gamma_multiply(0.6)
				};
				let glyph = ui.painter().layout_no_wrap(glyph.to_owned(), font, color);
				let gap = 8.0;
				let width = glyph.size().x + gap + text.size().x;
				let left = rect.center().x - width / 2.0;
				let top = |height: f32| rect.center().y - height / 2.0;

				ui.painter()
					.galley(egui::pos2(left, top(glyph.size().y)), glyph.clone(), color);
				ui.painter().galley(
					egui::pos2(left + glyph.size().x + gap, top(text.size().y)),
					text,
					ink,
				);
			}
			None => {
				ui.painter().galley(
					egui::pos2(
						rect.center().x - text.size().x / 2.0,
						rect.center().y - text.size().y / 2.0,
					),
					text,
					ink,
				);
			}
		}
	}

	if enabled {
		response.on_hover_cursor(egui::CursorIcon::PointingHand)
	} else {
		response
	}
}
