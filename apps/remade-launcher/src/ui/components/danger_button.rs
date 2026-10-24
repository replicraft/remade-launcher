//! A red button for actions that discard something.

use egui::{Button, Color32, Response, RichText, Stroke, Ui};

use crate::ui::theme;

pub(crate) fn danger_button(ui: &mut Ui, label: &str) -> Response {
	let button = Button::new(
		RichText::new(label)
			.color(Color32::WHITE)
			.family(theme::semibold()),
	)
	.fill(theme::palette(ui).danger)
	.stroke(Stroke::NONE);

	ui.add(button)
}
