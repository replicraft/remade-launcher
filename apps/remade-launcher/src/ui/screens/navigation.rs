//! The sidebar: the brand and a short list of places. It can collapse to a
//! strip of icons.
//!
//! Adding an app from GitHub is an action that opens its own dialog, so it
//! sits in the list but never becomes the current page.

use egui::{Frame, Margin, TextureHandle, Ui};

use super::Screen;
use crate::ui::components::{brand, icon_button, nav_item};
use crate::ui::{icons, theme};

pub(crate) struct Sidebar<'a> {
	pub(crate) logo: &'a TextureHandle,
	pub(crate) current: Screen,
	/// How many apps have an update waiting.
	pub(crate) updates: usize,
	pub(crate) collapsed: bool,
}

pub(crate) enum NavAction {
	None,
	Go(Screen),
	/// Open the Add from GitHub dialog.
	AddFromGithub,
	ToggleCollapsed,
}

pub(crate) fn navigation(ui: &mut Ui, sidebar: Sidebar<'_>) -> NavAction {
	let mut action = NavAction::None;
	let compact = sidebar.collapsed;
	let margin = if compact {
		Margin::symmetric(10, 16)
	} else {
		Margin::symmetric(14, 16)
	};

	Frame::new().inner_margin(margin).show(ui, |ui| {
		ui.spacing_mut().item_spacing.y = 6.0;

		if compact {
			// Everything in the strip shares one centre line.
			ui.vertical_centered(|ui| {
				ui.image((sidebar.logo.id(), egui::vec2(34.0, 34.0)));

				if icon_button(ui, icons::SIDEBAR, "Show sidebar", 34.0).clicked() {
					action = NavAction::ToggleCollapsed;
				}
			});
		} else {
			ui.horizontal(|ui| {
				brand(ui, sidebar.logo, false);

				ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
					if icon_button(ui, icons::SIDEBAR, "Hide sidebar", 28.0).clicked() {
						action = NavAction::ToggleCollapsed;
					}
				});
			});
		}

		ui.add_space(2.0);
		ui.separator();
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
			if nav_item(
				ui,
				icon,
				screen.title(),
				sidebar.current == screen,
				badge,
				compact,
			)
			.clicked()
			{
				action = NavAction::Go(screen);
			}
		}

		if nav_item(ui, icons::GITHUB, "Add from GitHub", false, None, compact).clicked() {
			action = NavAction::AddFromGithub;
		}

		ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
			if nav_item(
				ui,
				icons::GEAR,
				Screen::Settings.title(),
				sidebar.current == Screen::Settings,
				None,
				compact,
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
