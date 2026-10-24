//! A red button for actions that discard something.

use egui::{Button, Color32, Response, RichText, Stroke, Ui};

use crate::ui::theme;

/// Lighter under the pointer, darker while pressed, so it never looks dead.
pub(crate) fn danger_button(ui: &mut Ui, label: &str) -> Response {
	danger_button_enabled(ui, label, true)
}

pub(crate) fn danger_button_enabled(ui: &mut Ui, label: &str, enabled: bool) -> Response {
	let base = theme::palette(ui).danger;
	let hover = base.lerp_to_gamma(Color32::WHITE, 0.16);
	let pressed = base.lerp_to_gamma(Color32::BLACK, 0.22);

	ui.scope(|ui| {
		let widgets = &mut ui.visuals_mut().widgets;

		for (state, fill) in [
			(&mut widgets.inactive, base),
			(&mut widgets.hovered, hover),
			(&mut widgets.active, pressed),
		] {
			state.weak_bg_fill = fill;
			state.bg_fill = fill;
			state.bg_stroke = Stroke::NONE;
			state.fg_stroke.color = Color32::WHITE;
		}

		ui.add_enabled(
			enabled,
			Button::new(
				RichText::new(label)
					.color(Color32::WHITE)
					.family(theme::semibold()),
			),
		)
	})
	.inner
}
