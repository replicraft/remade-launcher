//! Library: the apps you have installed.

mod card;
mod controls;
mod status;

use std::collections::HashMap;

use egui::{RichText, ScrollArea, Ui};
use launcher_core::{InstalledApp, UpdateStatus};
use launcher_model::{InstallationId, UpdateMode};
use launcher_storage::AppPolicy;

use self::card::{CARD_HEIGHT, Item, grid_card, list_row};
pub(crate) use self::controls::LibraryState;
use self::controls::{Show, Sort, arrange};
pub(crate) use self::status::status_text;
use super::{Action, page, unavailable};
use crate::ui::components::{ViewMode, card_grid, dropdown, search_bar, view_toggle};
use crate::ui::theme;

/// Live progress for an operation, from engine events.
pub(crate) struct Progress {
	pub(crate) label: String,
	pub(crate) fraction: Option<f32>,
	/// Stopping is still safe: nothing has moved into place yet.
	pub(crate) stoppable: bool,
}

pub(crate) struct LibraryView<'a> {
	pub(crate) apps: &'a [InstalledApp],
	pub(crate) busy: &'a HashMap<InstallationId, Progress>,
	pub(crate) statuses: &'a HashMap<InstallationId, UpdateStatus>,
	pub(crate) policies: &'a HashMap<InstallationId, AppPolicy>,
	pub(crate) mode: UpdateMode,
	pub(crate) state: &'a mut LibraryState,
	pub(crate) view: &'a mut ViewMode,
	/// Demo apps are made up, so their links go to a placeholder page.
	pub(crate) demo: bool,
}

const CARD_MIN_WIDTH: f32 = 320.0;

pub(crate) fn library(ui: &mut Ui, view: &mut LibraryView<'_>) -> Option<Action> {
	let mut action = None;

	page(ui, "Library", |ui| {
		if view.apps.is_empty() && view.busy.is_empty() {
			unavailable(
				ui,
				"Nothing installed yet",
				"Apps you install with Remade Launcher will appear here. Find some in Explore.",
			);

			return;
		}

		search_bar(ui, "Search your apps", &mut view.state.search);
		ui.add_space(8.0);

		ui.horizontal_wrapped(|ui| {
			dropdown(
				ui,
				"library-sort",
				"Sort",
				&mut view.state.sort,
				&Sort::OPTIONS,
			);
			dropdown(
				ui,
				"library-show",
				"Show",
				&mut view.state.show,
				&Show::OPTIONS,
			);

			ui.label(
				RichText::new(view.mode.label())
					.small()
					.color(theme::muted(ui)),
			);

			ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
				view_toggle(ui, view.view);
			});
		});

		ui.add_space(8.0);

		let shown = arrange(view.apps, view.statuses, view.state);

		if shown.is_empty() {
			ui.label(RichText::new("No installed app matches.").color(theme::muted(ui)));
		}

		let items: Vec<Item<'_>> = shown
			.into_iter()
			.map(|app| Item {
				app,
				progress: view.busy.get(&app.id),
				status: view.statuses.get(&app.id),
				mode: view.mode,
				link: if view.demo {
					"https://example.com".to_owned()
				} else {
					format!("https://github.com/{}", app.repository)
				},
				pinned: view
					.policies
					.get(&app.id)
					.is_some_and(|policy| policy.pinned_release_id.is_some()),
			})
			.collect();

		ScrollArea::vertical()
			.auto_shrink([false, false])
			.show(ui, |ui| match *view.view {
				ViewMode::Grid => {
					card_grid(ui, items.len(), CARD_MIN_WIDTH, CARD_HEIGHT, |ui, index| {
						action = grid_card(ui, &items[index]).or_else(|| action.take());
					});
				}
				ViewMode::List => {
					for item in &items {
						action = list_row(ui, item).or_else(|| action.take());
					}
				}
			});
	});

	action
}
