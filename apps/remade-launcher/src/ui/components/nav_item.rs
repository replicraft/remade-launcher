//! One row of the sidebar: an icon, a label, and an optional count.

use egui::{Align2, FontId, Role, Sense, Ui, WidgetInfo, vec2};

use super::count_badge;
use crate::ui::theme;

/// `compact` draws the icon alone, centered, with the label as hover text.
pub(crate) fn nav_item(
	ui: &mut Ui,
	icon: &str,
	label: &str,
	selected: bool,
	badge: Option<usize>,
	compact: bool,
) -> egui::Response {
	let palette = theme::palette(ui);
	let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), 38.0), Sense::click());

	response.widget_info(|| WidgetInfo::labeled(Role::Button, true, label));

	if ui.is_rect_visible(rect) {
		let painter = ui.painter();

		if selected {
			painter.rect_filled(rect, 8.0, palette.selected);
		} else if response.hovered() {
			painter.rect_filled(rect, 8.0, palette.surface_hover);
		}

		if response.has_focus() {
			painter.rect_stroke(
				rect,
				8.0,
				egui::Stroke::new(1.5, palette.accent),
				egui::StrokeKind::Inside,
			);
		}

		let icon_color = if selected {
			palette.text
		} else {
			palette.text_muted
		};
		let font = if selected {
			FontId::new(14.5, theme::semibold())
		} else {
			FontId::proportional(14.5)
		};

		if compact {
			painter.text(
				rect.center(),
				Align2::CENTER_CENTER,
				icon,
				FontId::proportional(20.0),
				icon_color,
			);

			if badge.is_some() {
				painter.circle_filled(rect.center() + vec2(10.0, -10.0), 4.5, palette.accent);
			}
		} else {
			let middle = rect.left_center();

			painter.text(
				middle + vec2(14.0, 0.0),
				Align2::LEFT_CENTER,
				icon,
				FontId::proportional(18.0),
				icon_color,
			);
			painter.text(
				middle + vec2(44.0, 0.0),
				Align2::LEFT_CENTER,
				label,
				font,
				palette.text,
			);

			if let Some(count) = badge {
				count_badge::paint(
					painter,
					palette,
					rect.right_center() - vec2(24.0, 0.0),
					count,
				);
			}
		}
	}

	if compact {
		return response
			.on_hover_text(label)
			.on_hover_cursor(egui::CursorIcon::PointingHand);
	}

	response.on_hover_cursor(egui::CursorIcon::PointingHand)
}
