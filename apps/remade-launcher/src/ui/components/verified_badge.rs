//! The seal beside an app's name: checked by Remade, or added by the person.

use egui::{Response, RichText, Ui};

use crate::ui::{icons, theme};

/// A blue check for apps in the reviewed catalog, a muted warning seal for
/// ones the person added themselves. Hovering says which, and why.
pub(crate) fn verified_badge(ui: &mut Ui, reviewed: bool) -> Response {
	let palette = theme::palette(ui);

	let (icon, color, tip) = if reviewed {
		(icons::SEAL_CHECK, palette.accent, t!("common-reviewed"))
	} else {
		(
			icons::SEAL_WARNING,
			palette.text_muted,
			t!("common-not-reviewed"),
		)
	};

	ui.label(RichText::new(icon).size(16.0).color(color))
		.on_hover_text(tip)
}
