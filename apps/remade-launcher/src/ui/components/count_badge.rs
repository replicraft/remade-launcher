//! The small rounded count on a sidebar row.

use egui::{Align2, FontId, Painter, Rect, vec2};

use crate::ui::theme::{self, Palette};

/// Paints `count` centered on `center`. Counts above 99 read "99+".
pub(crate) fn paint(painter: &Painter, palette: &Palette, center: egui::Pos2, count: usize) {
	let text = if count > 99 {
		"99+".to_owned()
	} else {
		count.to_string()
	};
	let size = vec2(if text.len() > 1 { 30.0 } else { 24.0 }, 22.0);
	let rect = Rect::from_center_size(center, size);

	painter.rect_filled(rect, 11.0, palette.accent.gamma_multiply(0.22));
	painter.text(
		rect.center(),
		Align2::CENTER_CENTER,
		text,
		FontId::new(12.0, theme::semibold()),
		palette.accent,
	);
}
