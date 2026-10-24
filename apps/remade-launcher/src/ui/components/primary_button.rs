//! The big call-to-action button, such as Install.

use egui::{Button, Color32, Response, RichText, Stroke, Ui, Vec2};

use crate::ui::theme;

/// Filled with the accent colour, lighter under the pointer and darker while
/// pressed. A disabled one is dimmed and explains itself elsewhere.
pub(crate) fn primary_button(ui: &mut Ui, label: &str, enabled: bool, size: Vec2) -> Response {
	let palette = theme::palette(ui);
	let base = palette.accent;
	let hover = base.lerp_to_gamma(Color32::WHITE, 0.14);
	let pressed = base.lerp_to_gamma(Color32::BLACK, 0.18);
	// Dark text on the light blue of the dark theme, white on the deeper blue
	// of the light theme, so the label always reads.
	let text = if ui.visuals().dark_mode {
		Color32::from_rgb(0x0B, 0x0B, 0x0C)
	} else {
		Color32::WHITE
	};

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
			state.corner_radius = 12.into();
		}

		ui.add_enabled(
			enabled,
			Button::new(
				RichText::new(label)
					.size(17.0)
					.color(text)
					.family(theme::semibold()),
			)
			.min_size(size),
		)
	})
	.inner
}
