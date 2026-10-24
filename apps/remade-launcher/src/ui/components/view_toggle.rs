//! A two-way switch between a grid of cards and a list of rows.

use egui::{Align2, FontId, Role, Sense, Ui, WidgetInfo, vec2};

use crate::ui::{icons, theme};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum ViewMode {
	#[default]
	Grid,
	List,
}

const CELL: f32 = 32.0;

/// Draws both choices side by side. Returns true when the mode changed.
pub(crate) fn view_toggle(ui: &mut Ui, mode: &mut ViewMode) -> bool {
	let palette = theme::palette(ui);
	let before = *mode;

	let (rect, _) = ui.allocate_exact_size(vec2(CELL * 2.0 + 4.0, CELL), Sense::hover());

	ui.painter().rect_filled(rect, 8.0, palette.surface);
	ui.painter().rect_stroke(
		rect,
		8.0,
		egui::Stroke::new(1.0, palette.border),
		egui::StrokeKind::Inside,
	);

	for (index, (choice, icon, label)) in [
		(ViewMode::Grid, icons::GRID, "Grid view"),
		(ViewMode::List, icons::LIST, "List view"),
	]
	.into_iter()
	.enumerate()
	{
		let cell = egui::Rect::from_min_size(
			rect.min + vec2(2.0 + index as f32 * CELL, 2.0),
			vec2(CELL, CELL - 4.0),
		);
		let response = ui.allocate_rect(cell, Sense::click());
		let chosen = *mode == choice;

		response.widget_info(|| WidgetInfo::selected(Role::RadioButton, true, chosen, label));

		if chosen {
			ui.painter().rect_filled(cell, 6.0, palette.selected);
		} else if response.hovered() {
			ui.painter().rect_filled(cell, 6.0, palette.surface_hover);
		}

		let color = if chosen {
			palette.text
		} else {
			palette.text_muted
		};

		ui.painter().text(
			cell.center(),
			Align2::CENTER_CENTER,
			icon,
			FontId::proportional(17.0),
			color,
		);

		if response
			.on_hover_text(label)
			.on_hover_cursor(egui::CursorIcon::PointingHand)
			.clicked()
		{
			*mode = choice;
		}
	}

	*mode != before
}
