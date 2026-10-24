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

/// A dropdown whose first choice means "no filter". `current` is `None` for
/// that choice. Returns true when the choice changed.
pub(crate) fn choice_dropdown<T: Clone + PartialEq>(
	ui: &mut Ui,
	id: &str,
	label: &str,
	current: &mut Option<T>,
	all: &str,
	options: &[(T, String)],
) -> bool {
	let shown = current
		.as_ref()
		.and_then(|chosen| options.iter().find(|(value, _)| value == chosen))
		.map_or(all, |(_, name)| name.as_str());
	let mut changed = false;

	ComboBox::from_id_salt(id)
		.selected_text(format!("{label}: {shown}"))
		.show_ui(ui, |ui| {
			if ui.selectable_label(current.is_none(), all).clicked() && current.is_some() {
				*current = None;
				changed = true;
			}

			for (value, name) in options {
				let chosen = current.as_ref() == Some(value);

				if ui.selectable_label(chosen, name).clicked() && !chosen {
					*current = Some(value.clone());
					changed = true;
				}
			}
		});

	changed
}
