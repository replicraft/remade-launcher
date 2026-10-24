//! One setting: its name and explanation on the left, its control on the right.

use egui::{Align, Layout, RichText, Ui};

use crate::ui::theme;

/// Narrow windows stack the two sides instead of squeezing them.
const SIDE_BY_SIDE_FROM: f32 = 520.0;

pub(crate) fn setting_row(
	ui: &mut Ui,
	title: &str,
	description: Option<&str>,
	control: impl FnOnce(&mut Ui),
) {
	let palette = theme::palette(ui);
	let width = ui.available_width();

	let describe = |ui: &mut Ui| {
		ui.label(RichText::new(title).family(theme::semibold()));

		if let Some(description) = description {
			ui.add(
				egui::Label::new(RichText::new(description).small().color(palette.text_muted))
					.wrap(),
			);
		}
	};

	if width < SIDE_BY_SIDE_FROM {
		ui.vertical(|ui| {
			describe(ui);
			ui.add_space(4.0);
			control(ui);
		});
	} else {
		ui.horizontal_top(|ui| {
			ui.allocate_ui_with_layout(
				egui::vec2(width * 0.5 - 12.0, 0.0),
				Layout::top_down(Align::Min),
				|ui| {
					ui.set_width(width * 0.5 - 12.0);
					describe(ui);
				},
			);

			ui.with_layout(Layout::top_down(Align::Min), control);
		});
	}

	ui.add_space(10.0);
}
