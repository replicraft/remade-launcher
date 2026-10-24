//! The button row at the foot of a confirmation dialog.
//!
//! Cancel sits at the right edge, where the eye ends, with the action beside
//! it on the left.

use egui::{Align, Layout, Ui};

use super::danger_button::danger_button_enabled;

pub(crate) enum Choice {
	Pending,
	Cancel,
	Confirm,
}

pub(crate) struct Confirm<'a> {
	pub(crate) label: &'a str,
	/// Draw the action in red because it discards something.
	pub(crate) destructive: bool,
	pub(crate) enabled: bool,
}

pub(crate) fn dialog_buttons(ui: &mut Ui, confirm: &Confirm<'_>) -> Choice {
	let mut choice = Choice::Pending;

	ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
		if ui.button("Cancel").clicked() {
			choice = Choice::Cancel;
		}

		let clicked = if confirm.destructive {
			danger_button_enabled(ui, confirm.label, confirm.enabled).clicked()
		} else {
			ui.add_enabled(confirm.enabled, egui::Button::new(confirm.label))
				.clicked()
		};

		if clicked {
			choice = Choice::Confirm;
		}
	});

	choice
}
