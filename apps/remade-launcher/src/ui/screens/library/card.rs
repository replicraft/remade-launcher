//! How an installed app looks as a card or as a list row.

use egui::{Align, Layout, ProgressBar, Rect, RichText, Sense, Ui, UiBuilder, pos2, vec2};
use launcher_core::{InstalledApp, UpdateStatus};
use launcher_model::UpdateMode;

use super::Progress;
use super::status::status_text;
use crate::ui::components::{app_tile, icon_button, verified_badge};
use crate::ui::screens::Action;
use crate::ui::{icons, theme};

pub(super) const CARD_HEIGHT: f32 = 196.0;
pub(super) const ROW_HEIGHT: f32 = 84.0;

/// Everything one app's card or row needs.
pub(super) struct Item<'a> {
	pub(super) app: &'a InstalledApp,
	pub(super) progress: Option<&'a Progress>,
	pub(super) status: Option<&'a UpdateStatus>,
	pub(super) mode: UpdateMode,
	pub(super) pinned: bool,
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

/// Name with its seal, and the repository as a link to its GitHub page.
fn title(ui: &mut Ui, app: &InstalledApp) {
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

		ui.hyperlink_to(
			RichText::new(&app.repository).small(),
			format!("https://github.com/{}", app.repository),
		);
	});
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
fn actions(ui: &mut Ui, item: &Item<'_>) -> Option<Action> {
	let mut action = None;
	let app = item.app;

	if let Some(progress) = item.progress {
		ui.horizontal(|ui| {
			let bar = match progress.fraction {
				Some(fraction) => ProgressBar::new(fraction).text(&progress.label),
				None => ProgressBar::new(0.0).animate(true).text(&progress.label),
			};

			ui.add(bar.desired_width((ui.available_width() - 56.0).max(60.0)));

			if progress.stoppable && ui.small_button("Stop").clicked() {
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
				egui::Button::new(format!("{}  Open", icons::PLAY)),
			)
			.clicked()
		{
			action = Some(Action::Launch(app.id));
		}

		if app.curated && !item.pinned {
			let label = if update_ready {
				format!("{}  Update", icons::ARROW_CLOCKWISE)
			} else {
				"Check".to_owned()
			};

			if ui.button(label).clicked() {
				action = Some(Action::Update(app.id));
			}
		}

		ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
			if icon_button(ui, icons::TRASH, "Uninstall", 28.0).clicked() {
				action = Some(Action::Uninstall(app.clone()));
			}

			if app.curated && icon_button(ui, icons::GEAR, "Update settings", 28.0).clicked() {
				action = Some(Action::OpenAppSettings(app.id));
			}
		});
	});

	action
}

/// A card for the grid.
pub(super) fn grid_card(ui: &mut Ui, item: &Item<'_>) -> Option<Action> {
	let rect = ui.max_rect();
	let inner = rect.shrink(14.0);
	let mut action = None;

	surface(ui, rect);

	let mut top = ui.new_child(
		UiBuilder::new()
			.max_rect(inner)
			.layout(Layout::top_down(Align::Min)),
	);

	top.horizontal(|ui| {
		app_tile(ui, &item.app.name, 44.0);
		title(ui, item.app);
	});

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

	action = actions(&mut bottom, item).or(action);
	ui.allocate_rect(rect, Sense::hover());

	action
}

/// A single row for list view.
pub(super) fn list_row(ui: &mut Ui, item: &Item<'_>) -> Option<Action> {
	let rect = Rect::from_min_size(ui.cursor().min, vec2(ui.available_width(), ROW_HEIGHT));
	let inner = rect.shrink2(vec2(14.0, 12.0));
	let mut action = None;

	surface(ui, rect);

	let mut row = ui.new_child(
		UiBuilder::new()
			.max_rect(inner)
			.layout(Layout::left_to_right(Align::Center)),
	);

	app_tile(&mut row, &item.app.name, 48.0);

	row.allocate_ui_with_layout(vec2(220.0, 56.0), Layout::top_down(Align::Min), |ui| {
		ui.set_width(220.0);
		title(ui, item.app);
		ui.label(
			RichText::new(format!(
				"{} · installed {}",
				item.app.version,
				launcher_model::local_time::date(item.app.installed_at)
			))
			.small()
			.color(theme::muted(ui)),
		);
	});

	let status_width = (inner.width() - 48.0 - 220.0 - 340.0).max(120.0);

	row.allocate_ui_with_layout(
		vec2(status_width, 56.0),
		Layout::top_down(Align::Min),
		|ui| {
			ui.set_width(status_width);
			status_line(ui, item);
		},
	);

	row.allocate_ui_with_layout(vec2(330.0, 32.0), Layout::top_down(Align::Min), |ui| {
		ui.set_width(330.0);
		action = actions(ui, item);
	});

	ui.allocate_rect(rect, Sense::hover());
	ui.add_space(8.0);

	action
}
