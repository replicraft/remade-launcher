//! How an installed app looks as a card or as a list row.

use egui::{Align, Layout, ProgressBar, Rect, RichText, Sense, Ui, UiBuilder, pos2, vec2};
use launcher_core::{InstalledApp, UpdateStatus};
use launcher_model::UpdateMode;

use super::Progress;
use super::status::status_text;
use crate::ui::components::{app_tile_for, icon_button, verified_badge};
use crate::ui::screens::Action;
use crate::ui::{icons, theme};

pub(super) const CARD_HEIGHT: f32 = 212.0;
pub(super) const ROW_HEIGHT: f32 = 106.0;

/// Everything one app's card or row needs.
pub(super) struct Item<'a> {
	pub(super) app: &'a InstalledApp,
	pub(super) progress: Option<&'a Progress>,
	pub(super) status: Option<&'a UpdateStatus>,
	pub(super) mode: UpdateMode,
	pub(super) pinned: bool,
	/// Where the repository link goes.
	pub(super) link: String,
}

fn surface(ui: &mut Ui, rect: Rect) {
	let palette = theme::palette(ui);

	ui.painter().rect_filled(rect, 12.0, palette.surface);
	ui.painter().rect_stroke(
		rect,
		12.0,
		egui::Stroke::new(1.0, palette.border),
		egui::StrokeKind::Inside,
	);
}

/// Name with its seal, and the repository as a link to its page.
fn title(ui: &mut Ui, item: &Item<'_>) {
	let app = item.app;
	let palette = theme::palette(ui);

	ui.vertical(|ui| {
		ui.spacing_mut().item_spacing.y = 1.0;

		ui.horizontal(|ui| {
			ui.add(
				egui::Label::new(
					RichText::new(&app.name)
						.size(15.5)
						.family(theme::semibold()),
				)
				.truncate(),
			);
			verified_badge(ui, app.curated);
		});

		let link = ui
			.add(
				egui::Label::new(
					RichText::new(&app.repository)
						.small()
						.color(palette.accent)
						.underline(),
				)
				.sense(Sense::click()),
			)
			.on_hover_text(item.link.as_str())
			.on_hover_cursor(egui::CursorIcon::PointingHand);

		if link.clicked() {
			ui.ctx()
				.open_url(egui::OpenUrl::new_tab(item.link.as_str()));
		}
	});
}

/// How tall the status text comes out at `width`.
fn status_height(ui: &Ui, item: &Item<'_>, width: f32) -> f32 {
	let Some(status) = item.status else {
		return 0.0;
	};

	let (text, _) = status_text(status, item.mode);

	egui::WidgetText::from(RichText::new(text).small())
		.into_galley(
			ui,
			Some(egui::TextWrapMode::Wrap),
			width,
			egui::TextStyle::Body,
		)
		.size()
		.y
}

fn status_line(ui: &mut Ui, item: &Item<'_>) {
	let Some(status) = item.status else {
		return;
	};

	let (text, attention) = status_text(status, item.mode);
	let color = if attention {
		ui.visuals().warn_fg_color
	} else {
		theme::muted(ui)
	};

	ui.add(egui::Label::new(RichText::new(text).small().color(color)).wrap());
}

/// Open, Update, settings, and uninstall; or live progress with a Stop button.
/// `compact` drops the icons from the text buttons so a narrow card fits.
fn actions(ui: &mut Ui, item: &Item<'_>, compact: bool) -> Option<Action> {
	let mut action = None;
	let app = item.app;

	if let Some(progress) = item.progress {
		ui.horizontal(|ui| {
			let bar = match progress.fraction {
				Some(fraction) => ProgressBar::new(fraction).text(&progress.label),
				None => ProgressBar::new(0.0).animate(true).text(&progress.label),
			};

			ui.add(bar.desired_width((ui.available_width() - 56.0).max(60.0)));

			if progress.stoppable && ui.small_button(t!("library-stop")).clicked() {
				action = Some(Action::Cancel);
			}
		});

		return action;
	}

	let update_ready = item.status.is_some_and(|status| status.available.is_some());

	ui.horizontal(|ui| {
		if ui
			.add_enabled(
				app.launchable,
				egui::Button::new(if compact {
					t!("library-open")
				} else {
					format!("{}  {}", icons::PLAY, t!("library-open"))
				}),
			)
			.clicked()
		{
			action = Some(Action::Launch(app.id));
		}

		if app.curated && !item.pinned {
			let label = match (update_ready, compact) {
				(true, false) => format!("{}  {}", icons::ARROW_CLOCKWISE, t!("library-update")),
				(true, true) => t!("library-update"),
				(false, _) => t!("library-check"),
			};

			if ui.button(label).clicked() {
				action = Some(Action::Update(app.id));
			}
		}

		if !compact
			&& app.curated
			&& let Some(slug) = &app.catalog_slug
			&& ui.button(t!("library-view-listing")).clicked()
		{
			action = Some(Action::ViewListing(slug.clone()));
		}

		ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
			if icon_button(ui, icons::TRASH, &t!("library-uninstall"), 28.0).clicked() {
				action = Some(Action::Uninstall(app.clone()));
			}

			if app.curated
				&& icon_button(ui, icons::GEAR, &t!("library-update-settings"), 28.0).clicked()
			{
				action = Some(Action::OpenAppSettings(app.id));
			}
		});
	});

	action
}

/// A card for the grid.
pub(super) fn grid_card(ui: &mut Ui, item: &Item<'_>) -> Option<Action> {
	let rect = ui.max_rect();
	let inner = rect.shrink(18.0);
	let mut action = None;

	surface(ui, rect);

	let mut top = ui.new_child(
		UiBuilder::new()
			.max_rect(inner)
			.layout(Layout::top_down(Align::Min)),
	);

	top.horizontal(|ui| {
		// Leave the top-right corner for the listing button.
		ui.set_max_width(inner.width() - 110.0);
		app_tile_for(ui, item.app.catalog_slug.as_deref(), &item.app.name, 44.0);
		title(ui, item);
	});

	if item.app.curated
		&& let Some(slug) = &item.app.catalog_slug
	{
		let spot = Rect::from_min_size(pos2(inner.right() - 100.0, inner.top()), vec2(100.0, 26.0));

		if ui
			.put(
				spot,
				egui::Button::new(RichText::new(t!("library-view-listing")).small()),
			)
			.clicked()
		{
			action = Some(Action::ViewListing(slug.clone()));
		}
	}

	top.add_space(6.0);
	top.label(
		RichText::new(&item.app.version)
			.small()
			.color(theme::muted(&top)),
	);
	status_line(&mut top, item);

	let footer = Rect::from_min_max(
		pos2(inner.left(), inner.bottom() - 30.0),
		inner.right_bottom(),
	);
	let mut bottom = ui.new_child(
		UiBuilder::new()
			.max_rect(footer)
			.layout(Layout::top_down(Align::Min)),
	);

	action = actions(&mut bottom, item, true).or(action);
	ui.advance_cursor_after_rect(rect);

	action
}

/// A single row for list view.
///
/// The columns are placed by hand: a name block, a status block, and the
/// actions pinned to the right edge. Each is centred on the row's midline, so
/// nothing depends on how tall a nested layout happens to come out.
pub(super) fn list_row(ui: &mut Ui, item: &Item<'_>) -> Option<Action> {
	const TITLE_WIDTH: f32 = 250.0;
	const BLOCK_HEIGHT: f32 = 66.0;
	const BUTTON_HEIGHT: f32 = 32.0;

	let rect = Rect::from_min_size(ui.cursor().min, vec2(ui.available_width(), ROW_HEIGHT));
	let inner = rect.shrink2(vec2(14.0, 12.0));
	let middle = inner.center().y;
	let mut action = None;

	surface(ui, rect);

	let column = |ui: &mut Ui, left: f32, top: f32, width: f32, height: f32| {
		let area = Rect::from_min_size(pos2(left, top), vec2(width.max(1.0), height));

		ui.new_child(
			UiBuilder::new()
				.max_rect(area)
				.layout(Layout::top_down(Align::Min)),
		)
	};

	let mut tile = column(ui, inner.left(), middle - 24.0, 48.0, 48.0);
	app_tile_for(
		&mut tile,
		item.app.catalog_slug.as_deref(),
		&item.app.name,
		48.0,
	);

	let title_left = inner.left() + 48.0 + 14.0;
	let mut name = column(
		ui,
		title_left,
		middle - BLOCK_HEIGHT / 2.0,
		TITLE_WIDTH,
		BLOCK_HEIGHT,
	);

	name.spacing_mut().item_spacing.y = 1.0;
	title(&mut name, item);
	name.label(
		RichText::new(t!(
			"library-installed-line",
			version = item.app.version.as_str(),
			date = launcher_model::local_time::date(item.app.installed_at)
		))
		.small()
		.color(theme::muted(&name)),
	);

	// The same width for every row, so Open starts in the same place whether or
	// not the app has Check, View listing, and settings beside it.
	let actions_width = 440.0;
	let actions_left = inner.right() - actions_width;
	let status_left = title_left + TITLE_WIDTH + 12.0;
	let status_width = (actions_left - 12.0 - status_left).max(80.0);

	// The status sits in the middle of the card, whatever its length. Layout
	// cannot centre a block it has not measured, so measure the text first.
	let status_height = status_height(ui, item, status_width);
	let status_area = Rect::from_min_size(
		pos2(status_left, middle - status_height / 2.0),
		vec2(status_width, status_height),
	);
	let mut status = ui.new_child(
		UiBuilder::new()
			.max_rect(status_area)
			.layout(Layout::top_down(Align::Min)),
	);
	status_line(&mut status, item);

	let mut buttons = column(
		ui,
		actions_left,
		middle - BUTTON_HEIGHT / 2.0,
		actions_width,
		BUTTON_HEIGHT,
	);
	action = actions(&mut buttons, item, false).or(action);

	ui.advance_cursor_after_rect(rect);
	ui.add_space(8.0);

	action
}
