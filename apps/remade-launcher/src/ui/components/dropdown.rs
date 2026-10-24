//! A labelled dropdown over a short list of choices.

use egui::{ComboBox, Ui};

/// Shows `label: current` and a menu of `options`. Returns true when the
/// choice changed.
pub(crate) fn dropdown<T: Copy + PartialEq>(
	ui: &mut Ui,
	id: &str,
	label: &str,
	current: &mut T,
	options: &[(T, &str)],
) -> bool {
	let shown = options
		.iter()
		.find(|(value, _)| value == current)
		.map_or("", |(_, name)| *name);
	let mut changed = false;

	ComboBox::from_id_salt(id)
		.selected_text(format!("{label}: {shown}"))
		.show_ui(ui, |ui| {
			for (value, name) in options {
				if ui.selectable_label(current == value, *name).clicked() && current != value {
					*current = *value;
					changed = true;
				}
			}
		});

	changed
}
