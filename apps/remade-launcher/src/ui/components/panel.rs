//! A titled panel, for grouping related controls or facts.

use egui::{Frame, Margin, RichText, Ui};

use crate::ui::theme;

/// `description` is a short line of muted text under the title.
pub(crate) fn panel(
	ui: &mut Ui,
	title: &str,
	description: Option<&str>,
	contents: impl FnOnce(&mut Ui),
) {
	let palette = theme::palette(ui);

	Frame::new()
		.fill(palette.surface)
		.stroke(egui::Stroke::new(1.0, palette.border))
		.corner_radius(12)
		.inner_margin(Margin::same(20))
		.show(ui, |ui| {
			ui.set_width(ui.available_width());
			ui.label(RichText::new(title).size(17.0).family(theme::semibold()));

			if let Some(description) = description {
				ui.add_space(2.0);
				ui.add(
					egui::Label::new(RichText::new(description).small().color(palette.text_muted))
						.wrap(),
				);
			}

			ui.add_space(12.0);

			contents(ui);
		});
}
