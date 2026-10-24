//! A dropdown over a list of labelled choices, without a prefix label.

use egui::{ComboBox, Ui};

/// Shows the label of the current choice. Returns true when it changed.
pub(crate) fn choice_row<T: Copy + PartialEq>(
	ui: &mut Ui,
	id: &str,
	current: &mut T,
	options: &[(T, String)],
) -> bool {
	let shown = options
		.iter()
		.find(|(value, _)| value == current)
		.map_or("", |(_, label)| label.as_str());
	let mut changed = false;

	ComboBox::from_id_salt(id)
		.selected_text(shown)
		.width(220.0)
		.show_ui(ui, |ui| {
			for (value, label) in options {
				if ui.selectable_label(current == value, label).clicked() && current != value {
					*current = *value;
					changed = true;
				}
			}
		});

	changed
}
