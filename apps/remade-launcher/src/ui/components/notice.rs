//! A tinted strip with an icon and a message, optionally with one button.

use egui::{Align, Frame, Layout, Margin, RichText, Ui};

use crate::ui::theme;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Tone {
	/// Something is off, and the person may want to act.
	Warning,
}

/// Returns true when the button, if there is one, was clicked.
pub(crate) fn notice(
	ui: &mut Ui,
	tone: Tone,
	icon: &str,
	headline: &str,
	detail: Option<&str>,
	button: Option<&str>,
) -> bool {
	let palette = theme::palette(ui);
	let color = match tone {
		Tone::Warning => ui.visuals().warn_fg_color,
	};
	let mut clicked = false;

	Frame::new()
		.fill(color.gamma_multiply(0.10))
		.stroke(egui::Stroke::new(1.0, color.gamma_multiply(0.45)))
		.corner_radius(10)
		.inner_margin(Margin::symmetric(14, 10))
		.show(ui, |ui| {
			ui.set_width(ui.available_width());

			ui.horizontal(|ui| {
				ui.label(RichText::new(icon).size(20.0).color(color));

				ui.vertical(|ui| {
					ui.spacing_mut().item_spacing.y = 1.0;
					ui.label(RichText::new(headline).family(theme::semibold()));

					if let Some(detail) = detail {
						ui.label(RichText::new(detail).small().color(palette.text_muted));
					}
				});

				if let Some(label) = button {
					ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
						clicked = ui.button(label).clicked();
					});
				}
			});
		});

	clicked
}
