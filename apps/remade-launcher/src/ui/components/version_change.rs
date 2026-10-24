//! "Old v1.0.0 → New v1.1.0", for an update that is about to happen.

use egui::{RichText, Ui};

use crate::ui::{icons, theme};

pub(crate) fn version_change(ui: &mut Ui, old: &str, new: &str) {
	let palette = theme::palette(ui);

	ui.horizontal(|ui| {
		ui.spacing_mut().item_spacing.x = 8.0;

		ui.label(
			RichText::new(t!("common-version-old"))
				.small()
				.color(palette.text_muted),
		);
		ui.label(RichText::new(old).family(theme::semibold()));
		ui.label(
			RichText::new(icons::ARROW_RIGHT)
				.size(16.0)
				.color(palette.text_muted),
		);
		ui.label(
			RichText::new(t!("common-version-new"))
				.small()
				.color(palette.text_muted),
		);
		ui.label(
			RichText::new(new)
				.family(theme::semibold())
				.color(palette.accent),
		);
	});
}
