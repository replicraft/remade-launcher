//! A heading for a group of cards, with an optional note at the right edge.

use egui::{Align, Layout, RichText, Ui};

use crate::ui::theme;

pub(crate) fn section_title(ui: &mut Ui, icon: Option<&str>, title: &str, note: Option<&str>) {
	ui.add_space(14.0);

	ui.horizontal(|ui| {
		if let Some(icon) = icon {
			ui.label(RichText::new(icon).size(18.0).color(theme::muted(ui)));
		}

		ui.label(RichText::new(title).size(17.0).family(theme::semibold()));

		if let Some(note) = note {
			ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
				ui.label(RichText::new(note).small().color(theme::muted(ui)));
			});
		}
	});

	ui.add_space(4.0);
}
