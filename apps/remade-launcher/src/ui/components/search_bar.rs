//! A full-width search field with an icon and a clear button.

use egui::{
	Align, Align2, FontId, Frame, Key, Layout, Margin, RichText, Role, TextEdit, Ui, WidgetInfo,
};

use super::icon_button;
use crate::ui::{icons, theme};

const HEIGHT: f32 = 42.0;

/// Draws the field across the available width. Returns true when the text
/// changed. Escape clears it.
pub(crate) fn search_bar(ui: &mut Ui, hint: &str, text: &mut String) -> bool {
	let palette = theme::palette(ui);
	let mut changed = false;

	Frame::new()
		.fill(palette.surface)
		.stroke(egui::Stroke::new(1.0, palette.border))
		.corner_radius(10)
		.inner_margin(Margin::symmetric(12, 0))
		.show(ui, |ui| {
			ui.set_width(ui.available_width());
			ui.set_height(HEIGHT);

			ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
				ui.label(
					RichText::new(icons::SEARCH)
						.size(18.0)
						.color(palette.text_muted),
				);

				let clear_room = 32.0;
				let field = TextEdit::singleline(text)
					.hint_text(hint)
					.font(FontId::proportional(15.0))
					.frame(Frame::NONE)
					.margin(Margin::ZERO)
					.align(Align2::LEFT_CENTER)
					.desired_width(ui.available_width() - clear_room);
				let response = ui.add(field);

				response.widget_info(|| WidgetInfo::labeled(Role::TextInput, true, hint));
				changed = response.changed();

				if response.has_focus()
					&& ui.input(|input| input.key_pressed(Key::Escape))
					&& !text.is_empty()
				{
					text.clear();
					changed = true;
				}

				if !text.is_empty() && icon_button(ui, icons::X, "Clear search", 24.0).clicked() {
					text.clear();
					changed = true;
				}
			});
		});

	changed
}
