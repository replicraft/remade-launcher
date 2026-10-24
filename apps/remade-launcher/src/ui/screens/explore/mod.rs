//! Explore: browse the verified catalog.
//!
//! Search runs on the engine's blocking pool. This screen only sends the
//! query and draws whatever rows last came back, so typing never stalls a
//! frame. Sorting, the type filter, and the category filter apply to those
//! rows right here, which is instant.

mod banner;
mod cards;
mod detail;
mod filters;

use egui::{RichText, ScrollArea, Ui};
use launcher_core::{CatalogState, ExploreQuery, ExploreRow};

use self::banner::{banner, demo_banner};
use self::cards::{
	CATEGORY_TILE_HEIGHT, GRID_CARD_HEIGHT, app_count, category_tile, grid_card, list_row,
};
use self::detail::{demo_detail, detail};
use self::filters::{
	Filter, KindFilter, Sort, categories, category_label, featured, filter_demo, languages,
	platforms, recent, visible,
};
use super::{Action, page};
use crate::ui::commands::Commands;
use crate::ui::components::{
	ViewMode, card_grid, chip, choice_dropdown, dropdown, search_bar, section_title, view_toggle,
};
use crate::ui::{icons, theme};

const CARD_MIN_WIDTH: f32 = 270.0;
const FEATURED_MIN_WIDTH: f32 = 340.0;
const RECENT_COUNT: usize = 6;
/// Categories shown as chips before the rest are left to the tiles.
const CHIP_COUNT: usize = 8;

pub(crate) struct ExploreView {
	query: String,
	compatible_only: bool,
	filter: Filter,
	rows: Vec<ExploreRow>,
	/// The search, filters, and banner are showing. Folding them away leaves
	/// more room for the apps.
	controls_open: bool,
	selected: Option<String>,
	next_request: u64,
	latest_request: u64,
	asked: Option<SearchKey>,
}

/// Everything a result depends on. When it changes, ask again.
#[derive(PartialEq)]
struct SearchKey {
	text: String,
	compatible_only: bool,
	catalog: Option<[u8; 32]>,
}

impl ExploreView {
	pub(crate) fn new() -> Self {
		Self {
			query: String::new(),
			// Explore defaults to the current device. Browsing another
			// platform never changes where something would install.
			compatible_only: true,
			filter: Filter::default(),
			rows: Vec::new(),
			controls_open: true,
			selected: None,
			next_request: 0,
			latest_request: 0,
			asked: None,
		}
	}

	pub(crate) fn sort_key(&self) -> &'static str {
		self.filter.sort.key()
	}

	/// Restores a saved sort. An unknown name is ignored.
	pub(crate) fn set_sort_key(&mut self, key: &str) {
		if let Some(sort) = Sort::from_key(key) {
			self.filter.sort = sort;
		}
	}

	/// An app's display name, if it is among the rows.
	pub(crate) fn name_of(&self, slug: &str) -> Option<String> {
		self.rows
			.iter()
			.find(|row| row.slug == slug)
			.map(|row| row.name.clone())
	}

	/// Opens an app's page, such as from the Library.
	pub(crate) fn select(&mut self, slug: String) {
		self.selected = Some(slug);
	}

	/// Accepts results, ignoring any that a newer query has superseded.
	pub(crate) fn receive(&mut self, request: u64, rows: Vec<ExploreRow>) {
		if request == self.latest_request {
			self.rows = rows;
		}
	}

	/// Shows the catalog. `demo` replaces the engine's catalog with invented
	/// rows.
	pub(crate) fn show(
		&mut self,
		ui: &mut Ui,
		commands: &Commands,
		demo: Option<&[ExploreRow]>,
		mode: &mut ViewMode,
	) -> Option<Action> {
		let mut action = None;
		let state = commands.catalog_state();

		match demo {
			Some(rows) => self.rows = filter_demo(rows, &self.query, self.compatible_only),
			None => self.search_if_changed(&state, commands),
		}

		// An app's own page takes the whole width, so there is room for
		// screenshots and longer descriptions.
		if let Some(slug) = self.selected.clone() {
			page(ui, &t!("explore-title"), |ui| {
				let back = format!("{}  {}", icons::ARROW_LEFT, t!("explore-back"));

				if ui.button(back).clicked() {
					self.selected = None;
				}

				ui.add_space(10.0);

				action = self.detail_pane(ui, &slug, &state, demo.is_some());
			});

			return action;
		}

		page(ui, &t!("explore-title"), |ui| {
			self.controls_toggle(ui, &state);

			if self.controls_open {
				if demo.is_some() {
					ui.label(
						RichText::new(t!("explore-demo-note"))
							.small()
							.color(theme::muted(ui)),
					);
					demo_banner(ui, commands);
				} else {
					banner(ui, &state, commands);
				}

				search_bar(ui, &t!("explore-search-hint"), &mut self.query);
				ui.add_space(8.0);

				self.toolbar(ui, mode);
			}

			if state.catalog.is_none() && demo.is_none() {
				return;
			}

			ui.add_space(6.0);
			self.results(ui, *mode);
		});

		action
	}

	/// The button that folds the search and filters away and back.
	fn controls_toggle(&mut self, ui: &mut Ui, state: &CatalogState) {
		ui.horizontal(|ui| {
			let (icon, label) = if self.controls_open {
				(icons::CARET_UP, t!("explore-hide-controls"))
			} else {
				(icons::CARET_DOWN, t!("explore-show-controls"))
			};

			if ui.button(format!("{icon}  {label}")).clicked() {
				self.controls_open = !self.controls_open;
			}

			if self.controls_open {
				return;
			}

			if state.problem.is_some() {
				ui.label(
					RichText::new(icons::WARNING_CIRCLE)
						.size(18.0)
						.color(ui.visuals().warn_fg_color),
				)
				.on_hover_text(t!("explore-problem-hint"));
			}

			if !self.query.trim().is_empty() || !self.filter.is_open("") {
				ui.label(
					RichText::new(t!("explore-filters-active"))
						.small()
						.color(theme::muted(ui)),
				);
			}
		});

		ui.add_space(6.0);
	}

	fn toolbar(&mut self, ui: &mut Ui, mode: &mut ViewMode) {
		ui.horizontal_wrapped(|ui| {
			let sorts: Vec<(Sort, String)> = Sort::ALL
				.into_iter()
				.map(|sort| (sort, sort.label()))
				.collect();
			let kinds: Vec<(KindFilter, String)> = KindFilter::ALL
				.into_iter()
				.map(|kind| (kind, kind.label()))
				.collect();

			dropdown(
				ui,
				"explore-sort",
				&t!("explore-sort"),
				&mut self.filter.sort,
				&borrowed(&sorts),
			);
			dropdown(
				ui,
				"explore-kind",
				&t!("explore-type"),
				&mut self.filter.kind,
				&borrowed(&kinds),
			);

			let spoken: Vec<(String, String)> = languages(&self.rows)
				.into_iter()
				.map(|name| (name.clone(), name))
				.collect();
			let systems: Vec<(launcher_model::Os, String)> = platforms(&self.rows)
				.into_iter()
				.map(|os| (os, os.label().to_owned()))
				.collect();

			let all = t!("explore-all");

			if !spoken.is_empty() {
				choice_dropdown(
					ui,
					"explore-language",
					&t!("explore-language"),
					&mut self.filter.language,
					&all,
					&spoken,
				);
			}

			if !systems.is_empty() {
				choice_dropdown(
					ui,
					"explore-platform",
					&t!("explore-platform"),
					&mut self.filter.platform,
					&all,
					&systems,
				);
			}

			ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
				view_toggle(ui, mode);
			});
		});

		ui.add_space(6.0);
		ui.checkbox(&mut self.compatible_only, t!("explore-compatible-only"));

		let popular = categories(&self.rows);

		if popular.is_empty() {
			return;
		}

		ui.add_space(6.0);

		ui.horizontal_wrapped(|ui| {
			if chip(ui, &t!("explore-all"), self.filter.category.is_none()) {
				self.filter.category = None;
			}

			for (slug, _) in popular.iter().take(CHIP_COUNT) {
				let selected = self.filter.category.as_deref() == Some(slug.as_str());

				if chip(ui, &category_label(slug), selected) {
					self.filter.category = if selected { None } else { Some(slug.clone()) };
				}
			}
		});
	}

	fn results(&mut self, ui: &mut Ui, mode: ViewMode) {
		if self.rows.is_empty() {
			self.nothing_found(ui);

			return;
		}

		let open = self.filter.is_open(&self.query);
		let mut picked: Option<String> = None;
		let mut category: Option<String> = None;

		ScrollArea::vertical()
			.auto_shrink([false, false])
			.show(ui, |ui| {
				if open {
					self.sections(ui, mode, &mut picked, &mut category);
				}

				let shown = visible(&self.rows, &self.filter);
				let title = match &self.filter.category {
					Some(slug) => category_label(slug),
					None if open => t!("explore-all-apps"),
					None => t!("explore-results"),
				};

				section_title(ui, None, &title, Some(&app_count(shown.len())));

				if shown.is_empty() {
					ui.label(RichText::new(t!("explore-none-for-filters")).color(theme::muted(ui)));
				}

				self.draw(ui, mode, &shown, None, CARD_MIN_WIDTH, &mut picked);
			});

		if let Some(slug) = picked {
			self.selected = Some(slug);
		}

		if let Some(slug) = category {
			self.filter.category = Some(slug);
		}
	}

	/// The home sections: Featured, Recently added, and Categories.
	fn sections(
		&self,
		ui: &mut Ui,
		mode: ViewMode,
		picked: &mut Option<String>,
		category: &mut Option<String>,
	) {
		let starred = featured(&self.rows);

		if !starred.is_empty() {
			section_title(ui, Some(icons::STAR), &t!("explore-featured"), None);
			self.draw(ui, mode, &starred, None, FEATURED_MIN_WIDTH, picked);
		}

		let newest = recent(&self.rows, RECENT_COUNT);

		if !newest.is_empty() {
			section_title(ui, Some(icons::SPARKLE), &t!("explore-recent"), None);
			self.draw(ui, mode, &newest, Some(Note::Added), CARD_MIN_WIDTH, picked);
		}

		let groups = categories(&self.rows);

		if !groups.is_empty() {
			section_title(ui, Some(icons::GRID), &t!("explore-categories"), None);

			card_grid(
				ui,
				groups.len(),
				210.0,
				CATEGORY_TILE_HEIGHT,
				|ui, index| {
					let (slug, count) = &groups[index];

					if category_tile(ui, slug, *count, false) {
						*category = Some(slug.clone());
					}
				},
			);
		}
	}

	/// Draws `rows` as cards or as a list, recording a click in `picked`.
	fn draw(
		&self,
		ui: &mut Ui,
		mode: ViewMode,
		rows: &[&ExploreRow],
		note: Option<Note>,
		min_width: f32,
		picked: &mut Option<String>,
	) {
		let chosen = self.selected.as_deref();

		match mode {
			ViewMode::Grid => {
				card_grid(ui, rows.len(), min_width, GRID_CARD_HEIGHT, |ui, index| {
					let row = rows[index];
					let text = note.map(|note| note.text(row));

					if grid_card(ui, row, chosen == Some(row.slug.as_str()), text) {
						*picked = Some(row.slug.clone());
					}
				})
			}
			ViewMode::List => {
				for row in rows {
					if list_row(ui, row, chosen == Some(row.slug.as_str())) {
						*picked = Some(row.slug.clone());
					}

					ui.add_space(6.0);
				}
			}
		}
	}

	fn nothing_found(&self, ui: &mut Ui) {
		ui.add_space(16.0);
		ui.label(RichText::new(t!("explore-nothing")).color(theme::muted(ui)));

		if self.compatible_only {
			ui.label(RichText::new(t!("explore-nothing.hint")).color(theme::muted(ui)));
		}
	}

	fn detail_pane(
		&mut self,
		ui: &mut Ui,
		slug: &str,
		state: &CatalogState,
		demo: bool,
	) -> Option<Action> {
		if demo {
			return match self.rows.iter().find(|row| row.slug == slug).cloned() {
				Some(row) => demo_detail(ui, &row),
				None => {
					self.selected = None;

					None
				}
			};
		}

		let availability = self
			.rows
			.iter()
			.find(|row| row.slug == slug)
			.map(|row| row.availability.clone());

		match state
			.catalog
			.as_ref()
			.and_then(|catalog| catalog.project(slug))
		{
			Some(project) => detail(ui, slug, project, availability.as_ref()),
			None => {
				self.selected = None;

				None
			}
		}
	}

	fn search_if_changed(&mut self, state: &CatalogState, commands: &Commands) {
		let key = SearchKey {
			text: self.query.clone(),
			compatible_only: self.compatible_only,
			catalog: state.catalog.as_ref().map(|catalog| catalog.digest()),
		};

		if self.asked.as_ref() == Some(&key) {
			return;
		}

		self.next_request += 1;
		self.latest_request = self.next_request;

		commands.search(
			self.latest_request,
			ExploreQuery {
				text: key.text.clone(),
				compatible_only: key.compatible_only,
			},
		);

		self.asked = Some(key);
	}
}

/// Option labels as the dropdown wants them.
fn borrowed<T: Copy>(options: &[(T, String)]) -> Vec<(T, &str)> {
	options
		.iter()
		.map(|(value, label)| (*value, label.as_str()))
		.collect()
}

/// What a card's footer says in place of its category.
#[derive(Clone, Copy)]
enum Note {
	Added,
}

impl Note {
	fn text(self, row: &ExploreRow) -> String {
		match self {
			Self::Added => match row.added_at {
				Some(at) => t!("explore-added", date = launcher_model::local_time::date(at)),
				None => String::new(),
			},
		}
	}
}
