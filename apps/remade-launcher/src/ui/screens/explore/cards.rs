//! How a catalog app looks as a card, a list row, or a category tile.

use egui::text::LayoutJob;
use egui::{Align, Align2, FontId, Layout, Rect, RichText, Sense, Ui, UiBuilder, pos2, vec2};
use launcher_core::{Availability, ExploreRow};

use super::filters::category_label;
use crate::ui::components::{app_tile, app_tile_for, platform_badges, verified_badge};
use crate::ui::theme;

pub(super) const GRID_CARD_HEIGHT: f32 = 160.0;
pub(super) const LIST_ROW_HEIGHT: f32 = 88.0;
pub(super) const CATEGORY_TILE_HEIGHT: f32 = 64.0;

/// A short phrase for the corner of a card.
fn availability_short(availability: &Availability) -> String {
	match availability {
		Availability::ThisDevice => t!("explore-short-compatible"),
		Availability::NeedsNewerLauncher => t!("explore-short-newer-launcher"),
		Availability::OtherPlatforms(_) => t!("explore-short-other-platforms"),
		Availability::NoPackages => t!("explore-short-no-package"),
	}
}

/// Paints the card's background and returns its click response.
fn surface(ui: &mut Ui, rect: Rect, selected: bool) -> egui::Response {
	let palette = theme::palette(ui);
	let response = ui.allocate_rect(rect, Sense::click());

	let fill = if response.hovered() {
		palette.surface_hover
	} else {
		palette.surface
	};
	let outline = if selected {
		egui::Stroke::new(1.5, palette.accent)
	} else {
		egui::Stroke::new(1.0, palette.border)
	};

	ui.painter().rect_filled(rect, 12.0, fill);
	ui.painter()
		.rect_stroke(rect, 12.0, outline, egui::StrokeKind::Inside);

	response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// A card for the grid. `note` replaces the category in the footer, such as
/// "Added Oct 24, 2026". Returns true when clicked.
pub(super) fn grid_card(
	ui: &mut Ui,
	row: &ExploreRow,
	selected: bool,
	note: Option<String>,
) -> bool {
	let palette = theme::palette(ui);
	let rect = ui.max_rect();
	let response = surface(ui, rect, selected);
	let inner = rect.shrink(18.0);

	let mut content = ui.new_child(
		UiBuilder::new()
			.max_rect(inner)
			.layout(egui::Layout::top_down(egui::Align::Min)),
	);

	content.horizontal(|ui| {
		app_tile_for(ui, Some(&row.slug), &row.name, 44.0);

		ui.vertical(|ui| {
			ui.spacing_mut().item_spacing.y = 1.0;

			ui.horizontal(|ui| {
				ui.add(
					egui::Label::new(
						RichText::new(&row.name)
							.size(15.5)
							.family(theme::semibold()),
					)
					.truncate(),
				);
				verified_badge(ui, true);
			});
			ui.add(
				egui::Label::new(
					RichText::new(&row.repository)
						.small()
						.color(palette.text_muted),
				)
				.truncate(),
			);
		});
	});

	content.add_space(8.0);
	content.label(clamped(
		&row.summary,
		13.5,
		palette.text_muted,
		inner.width(),
	));

	// Where it runs, then what it is written in (or when it was added).
	let footer_area = Rect::from_min_max(
		pos2(inner.left(), inner.bottom() - 20.0),
		pos2(inner.right() - 120.0, inner.bottom()),
	);
	let mut footer = ui.new_child(
		UiBuilder::new()
			.max_rect(footer_area)
			.layout(Layout::left_to_right(Align::Center)),
	);

	if footer.available_width() >= badge_width(&row.platforms) + 60.0 {
		platform_badges(&mut footer, &row.platforms);
	}

	let detail = note.unwrap_or_default();

	if !detail.is_empty() {
		footer.add(
			egui::Label::new(
				RichText::new(format!("·  {detail}"))
					.small()
					.color(palette.text_muted),
			)
			.truncate(),
		);
	}

	ui.painter().text(
		inner.right_bottom(),
		Align2::RIGHT_BOTTOM,
		availability_short(&row.availability),
		FontId::proportional(12.0),
		if row.availability == Availability::ThisDevice {
			palette.text_muted
		} else {
			ui.visuals().warn_fg_color
		},
	);

	response.clicked()
}

/// A single-row version of the card for list view.
pub(super) fn list_row(ui: &mut Ui, row: &ExploreRow, selected: bool) -> bool {
	let palette = theme::palette(ui);
	let rect = Rect::from_min_size(ui.cursor().min, vec2(ui.available_width(), LIST_ROW_HEIGHT));
	let response = surface(ui, rect, selected);
	let inner = rect.shrink2(vec2(16.0, 0.0));
	let middle = rect.center().y;

	// Both columns are placed on the row's midline by hand, so the text has
	// the same room above and below.
	let mut tile = ui.new_child(
		UiBuilder::new()
			.max_rect(Rect::from_min_size(
				pos2(inner.left(), middle - 20.0),
				vec2(40.0, 40.0),
			))
			.layout(Layout::top_down(Align::Min)),
	);
	app_tile_for(&mut tile, Some(&row.slug), &row.name, 40.0);

	let text_left = inner.left() + 40.0 + 14.0;
	let text_area = Rect::from_min_size(
		pos2(text_left, middle - 31.0),
		vec2((inner.right() - text_left - 160.0).max(100.0), 44.0),
	);
	let mut text = ui.new_child(
		UiBuilder::new()
			.max_rect(text_area)
			.layout(Layout::top_down(Align::Min)),
	);

	text.spacing_mut().item_spacing.y = 2.0;

	text.horizontal(|ui| {
		ui.add(
			egui::Label::new(
				RichText::new(&row.name)
					.size(15.5)
					.family(theme::semibold()),
			)
			.truncate(),
		);
		verified_badge(ui, true);
		ui.label(
			RichText::new(&row.repository)
				.small()
				.color(palette.text_muted),
		);

		// The logos are an extra: they appear only if there is room left
		// after the name and repository, and never push past the card.
		if ui.available_width() >= badge_width(&row.platforms) {
			platform_badges(ui, &row.platforms);
		}
	});
	text.add(egui::Label::new(RichText::new(&row.summary).color(palette.text_muted)).truncate());

	ui.painter().text(
		inner.right_center(),
		Align2::RIGHT_CENTER,
		availability_short(&row.availability),
		FontId::proportional(12.0),
		if row.availability == Availability::ThisDevice {
			palette.text_muted
		} else {
			ui.visuals().warn_fg_color
		},
	);

	response.clicked()
}

/// A tile for browsing by category. Returns true when clicked.
pub(super) fn category_tile(ui: &mut Ui, slug: &str, count: usize, selected: bool) -> bool {
	let palette = theme::palette(ui);
	let rect = ui.max_rect();
	let response = surface(ui, rect, selected);
	let inner = rect.shrink2(vec2(14.0, 0.0));
	let label = category_label(slug);

	let mut content = ui.new_child(
		UiBuilder::new()
			.max_rect(inner)
			.layout(egui::Layout::left_to_right(egui::Align::Center)),
	);

	app_tile(&mut content, &label, 36.0);
	content.allocate_ui_with_layout(
		vec2((inner.width() - 48.0).max(60.0), 40.0),
		Layout::top_down(Align::Min),
		|ui| {
			ui.spacing_mut().item_spacing.y = 0.0;
			ui.label(RichText::new(&label).family(theme::semibold()));
			ui.label(
				RichText::new(app_count(count))
					.small()
					.color(palette.text_muted),
			);
		},
	);

	response.clicked()
}

/// "1 app" or "5 apps".
pub(super) fn app_count(count: usize) -> String {
	t!("explore-app-count", count = count as i64)
}

/// Summary text that stops after two lines with an ellipsis.
fn clamped(text: &str, size: f32, color: egui::Color32, width: f32) -> LayoutJob {
	let mut job = LayoutJob::simple(text.to_owned(), FontId::proportional(size), color, width);

	job.wrap.max_rows = 2;
	job.wrap.break_anywhere = false;

	job
}

/// About how wide the platform logos are, with their spacing.
fn badge_width(platforms: &[launcher_model::Os]) -> f32 {
	platforms.len() as f32 * 26.0
}
