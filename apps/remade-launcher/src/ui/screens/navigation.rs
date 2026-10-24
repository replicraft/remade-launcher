//! The sidebar: the brand and a short list of places.
//!
//! Adding an app from GitHub is an action that opens its own dialog, so it
//! sits in the list but never becomes the current page.

use egui::{Frame, Margin, TextureHandle, Ui};

use super::Screen;
use crate::ui::components::{brand, nav_item};
use crate::ui::{icons, theme};

pub(crate) struct Sidebar<'a> {
	pub(crate) logo: &'a TextureHandle,
	pub(crate) current: Screen,
	/// How many apps have an update waiting.
	pub(crate) updates: usize,
}

pub(crate) enum NavAction {
	None,
	Go(Screen),
	/// Open the Add from GitHub dialog.
	AddFromGithub,
}

pub(crate) fn navigation(ui: &mut Ui, sidebar: Sidebar<'_>) -> NavAction {
	let mut action = NavAction::None;

	Frame::new()
		.inner_margin(Margin::symmetric(14, 16))
		.show(ui, |ui| {
			ui.spacing_mut().item_spacing.y = 6.0;

			brand(ui, sidebar.logo);
			ui.add_space(8.0);

			for (screen, icon, badge) in [
				(Screen::Explore, icons::COMPASS, None),
				(Screen::Library, icons::BOOKS, None),
				(
					Screen::Updates,
					icons::DOWNLOAD,
					(sidebar.updates > 0).then_some(sidebar.updates),
				),
			] {
				if nav_item(ui, icon, screen.title(), sidebar.current == screen, badge).clicked() {
					action = NavAction::Go(screen);
				}
			}

			if nav_item(ui, icons::GITHUB, "Add from GitHub", false, None).clicked() {
				action = NavAction::AddFromGithub;
			}

			ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
				if nav_item(
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

/// The sidebar's own fill, which differs from the page.
pub(crate) fn fill(context: &egui::Context) -> egui::Color32 {
	theme::palette_for(context).sidebar
}
