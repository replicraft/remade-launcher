//! The sidebar: the brand, a search field, and a short list of places.
//!
//! Adding an app from GitHub is an action that opens its own dialog, so it
//! sits in the list but never becomes the current page.

use egui::{
	Align2, FontId, Frame, Id, Key, Margin, RichText, Role, Sense, TextEdit, TextureHandle, Ui,
	WidgetInfo,
};
use launcher_model::product;

use super::Screen;
use crate::{icons, theme};

/// The id of the search field, so the shortcut can focus it.
pub(crate) fn search_id() -> Id {
	Id::unique("sidebar-search")
}

pub(crate) struct Sidebar<'a> {
	pub(crate) logo: &'a TextureHandle,
	pub(crate) current: Screen,
	/// How many apps have an update waiting.
	pub(crate) updates: usize,
	pub(crate) search: &'a mut String,
}

pub(crate) enum NavAction {
	None,
	Go(Screen),
	/// Open the Add from GitHub dialog.
	AddFromGithub,
	/// The search text changed.
	Searched,
}

/// "⌘K" on a Mac, "Ctrl K" elsewhere.
pub(crate) fn shortcut_label() -> &'static str {
	if cfg!(target_os = "macos") {
		"⌘K"
	} else {
		"Ctrl K"
	}
}

pub(crate) fn navigation(ui: &mut Ui, sidebar: Sidebar<'_>) -> NavAction {
	let mut action = NavAction::None;

	Frame::new()
		.inner_margin(Margin::symmetric(14, 16))
		.show(ui, |ui| {
			ui.spacing_mut().item_spacing.y = 6.0;

			brand(ui, sidebar.logo);
			ui.add_space(8.0);

			if search_field(ui, sidebar.search) {
				action = NavAction::Searched;
			}

			ui.add_space(6.0);

			for (screen, icon, badge) in [
				(Screen::Explore, icons::COMPASS, None),
				(Screen::Library, icons::BOOKS, None),
				(
					Screen::Updates,
					icons::DOWNLOAD,
					(sidebar.updates > 0).then_some(sidebar.updates),
				),
			] {
				if item(ui, icon, screen.title(), sidebar.current == screen, badge).clicked() {
					action = NavAction::Go(screen);
				}
			}

			if item(ui, icons::GITHUB, "Add from GitHub", false, None).clicked() {
				action = NavAction::AddFromGithub;
			}

			ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
				if item(
					ui,
					icons::GEAR,
					Screen::Settings.title(),
					sidebar.current == Screen::Settings,
					None,
				)
				.clicked()
				{
					action = NavAction::Go(Screen::Settings);
				}

				ui.separator();
			});
		});

	action
}

fn brand(ui: &mut Ui, logo: &TextureHandle) {
	ui.horizontal(|ui| {
		ui.image((logo.id(), egui::vec2(34.0, 34.0)));

		ui.vertical(|ui| {
			ui.spacing_mut().item_spacing.y = 0.0;
			ui.label(
				RichText::new(product::NAME)
					.size(16.0)
					.family(theme::semibold()),
			);
			ui.label(
				RichText::new(product::TAGLINE)
					.size(11.0)
					.color(theme::muted(ui)),
			);
		});
	});

	ui.add_space(6.0);
	ui.separator();
}

/// The search field. Returns true when its text changed.
fn search_field(ui: &mut Ui, text: &mut String) -> bool {
	let palette = theme::palette(ui);
	let mut changed = false;

	Frame::new()
		.fill(palette.surface)
		.stroke(egui::Stroke::new(1.0, palette.border))
		.corner_radius(8)
		.inner_margin(Margin::symmetric(10, 4))
		.show(ui, |ui| {
			ui.horizontal(|ui| {
				ui.label(
					RichText::new(icons::SEARCH)
						.size(16.0)
						.color(palette.text_muted),
				);

				let width = ui.available_width() - 44.0;
				let field = TextEdit::singleline(text)
					.id(search_id())
					.hint_text("Search")
					.desired_width(width)
					.frame(Frame::NONE);
				let response = ui.add(field);

				response.widget_info(|| WidgetInfo::labeled(Role::TextInput, true, "Search apps"));
				changed = response.changed();

				// Escape clears the search and gives the keyboard back.
				if response.has_focus() && ui.input(|input| input.key_pressed(Key::Escape)) {
					text.clear();
					response.surrender_focus();
					changed = true;
				}

				if text.is_empty() {
					ui.label(
						RichText::new(shortcut_label())
							.size(11.0)
							.color(palette.text_muted),
					);
				}
			});
		});

	changed
}

/// One row of the list: an icon, a label, and an optional count.
fn item(
	ui: &mut Ui,
	icon: &str,
	label: &str,
	selected: bool,
	badge: Option<usize>,
) -> egui::Response {
	let palette = theme::palette(ui);
	let (rect, response) =
		ui.allocate_exact_size(egui::vec2(ui.available_width(), 38.0), Sense::click());

	response.widget_info(|| WidgetInfo::labeled(Role::Button, true, label));

	if ui.is_rect_visible(rect) {
		let painter = ui.painter();

		if selected {
			painter.rect_filled(rect, 8.0, palette.selected);
		} else if response.hovered() {
			painter.rect_filled(rect, 8.0, palette.surface_hover);
		}

		if response.has_focus() {
			painter.rect_stroke(
				rect,
				8.0,
				egui::Stroke::new(1.5, palette.accent),
				egui::StrokeKind::Inside,
			);
		}

		let color = if selected {
			palette.text
		} else {
			palette.text_muted
		};
		let middle = rect.left_center();

		painter.text(
			middle + egui::vec2(14.0, 0.0),
			Align2::LEFT_CENTER,
			icon,
			FontId::proportional(18.0),
			color,
		);
		painter.text(
			middle + egui::vec2(44.0, 0.0),
			Align2::LEFT_CENTER,
			label,
			if selected {
				FontId::new(14.5, theme::semibold())
			} else {
				FontId::proportional(14.5)
			},
			palette.text,
		);

		if let Some(count) = badge {
			let text = if count > 99 {
				"99+".to_owned()
			} else {
				count.to_string()
			};
			let size = egui::vec2(if text.len() > 1 { 30.0 } else { 24.0 }, 22.0);
			let badge_rect =
				egui::Rect::from_center_size(rect.right_center() - egui::vec2(24.0, 0.0), size);

			painter.rect_filled(badge_rect, 11.0, palette.accent.gamma_multiply(0.22));
			painter.text(
				badge_rect.center(),
				Align2::CENTER_CENTER,
				text,
				FontId::new(12.0, theme::semibold()),
				palette.accent,
			);
		}
	}

	response.on_hover_cursor(egui::CursorIcon::PointingHand)
}
