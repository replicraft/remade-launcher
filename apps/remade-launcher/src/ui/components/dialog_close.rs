//! The × in a dialog's top-right corner.

use egui::{Pos2, Rect, Ui, vec2};

use super::icon_button;
use crate::ui::icons;

const SIZE: f32 = 28.0;

/// Draws the close button over the top-right corner of the dialog's content,
/// which starts at `origin` and is `width` wide. Returns true when clicked.
///
/// Call it after the content, so it sits on top of whatever the title used.
pub(crate) fn dialog_close(ui: &mut Ui, origin: Pos2, width: f32) -> bool {
	let rect = Rect::from_min_size(
		Pos2::new(origin.x + width - SIZE, origin.y),
		vec2(SIZE, SIZE),
	);

	ui.put(rect, |ui: &mut Ui| icon_button(ui, icons::X, "Close", SIZE))
		.clicked()
}
