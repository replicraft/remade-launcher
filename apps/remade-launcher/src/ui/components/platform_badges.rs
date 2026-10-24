//! Small system logos for where an app runs, with the name on hover.

use egui::{RichText, Ui};
use launcher_model::Os;

use crate::ui::{icons, theme};

/// The Phosphor logo for a system, or `None` for ones it has no logo for.
fn logo(os: Os) -> Option<&'static str> {
	match os {
		Os::Windows => Some(icons::WINDOWS),
		Os::Macos => Some(icons::APPLE),
		Os::Linux => Some(icons::LINUX),
		_ => None,
	}
}

/// Draws a logo for each system, in a row. Systems without a logo are
/// written out as text so none is silently dropped.
pub(crate) fn platform_badges(ui: &mut Ui, platforms: &[Os]) {
	let color = theme::muted(ui);

	ui.horizontal(|ui| {
		ui.spacing_mut().item_spacing.x = 6.0;

		for os in platforms {
			match logo(*os) {
				Some(icon) => ui.label(RichText::new(icon).size(15.0).color(color)),
				None => ui.label(RichText::new(os.label()).small().color(color)),
			}
			.on_hover_text(os.label());
		}
	});
}
