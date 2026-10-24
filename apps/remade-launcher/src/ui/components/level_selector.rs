//! A choice among ordered levels, drawn like Tor Browser's security level:
//! a track with a stop for each level, filled up to the one chosen, and the
//! chosen level's explanation under its name.

use egui::{Color32, RichText, Sense, Stroke, Ui, pos2};

use crate::ui::theme;

/// Room for the track and its stops, left of the names.
const TRACK_WIDTH: f32 = 28.0;
const STOP_RADIUS: f32 = 5.0;
const CHOSEN_RADIUS: f32 = 7.0;

/// One level: what it is called, what it does, and whether it can be chosen.
pub(crate) struct Level<T> {
	pub(crate) value: T,
	pub(crate) title: String,
	pub(crate) description: String,
	/// Drawn as a warning, for a level that can do harm.
	pub(crate) warn: bool,
	/// A disabled level is shown but cannot be picked, and says why in `note`.
	pub(crate) enabled: bool,
	pub(crate) note: Option<String>,
}

/// Draws the levels in order. Returns the level the person just picked.
pub(crate) fn level_selector<T: Copy + PartialEq>(
	ui: &mut Ui,
	id: impl std::hash::Hash + std::fmt::Debug,
	current: T,
	levels: &[Level<T>],
) -> Option<T> {
	let palette = theme::palette(ui);
	let id = ui.make_persistent_id(id);
	let chosen = levels.iter().position(|level| level.value == current);
	let mut picked = None;
	let mut stops = Vec::with_capacity(levels.len());

	for (index, level) in levels.iter().enumerate() {
		let is_chosen = chosen == Some(index);

		let row = ui.horizontal_top(|ui| {
			ui.add_space(TRACK_WIDTH);
			ui.vertical(|ui| {
				let title = RichText::new(&level.title).family(theme::semibold());
				let title = match (level.enabled, level.warn) {
					(false, _) => title.color(palette.text_muted),
					(true, true) => title.color(ui.visuals().warn_fg_color),
					(true, false) => title,
				};

				ui.label(title);

				if is_chosen {
					ui.add(
						egui::Label::new(
							RichText::new(&level.description)
								.small()
								.color(palette.text_muted),
						)
						.wrap(),
					);
				}

				if let Some(note) = level.note.as_ref().filter(|_| is_chosen || !level.enabled) {
					ui.add(
						egui::Label::new(
							RichText::new(note)
								.small()
								.color(ui.visuals().warn_fg_color),
						)
						.wrap(),
					);
				}
			});
		});

		let rect = row.response.rect;
		let line = ui.text_style_height(&egui::TextStyle::Body);

		stops.push(pos2(
			rect.left() + TRACK_WIDTH / 2.0,
			rect.top() + line / 2.0 + 1.0,
		));

		let click = ui.interact(rect, id.with(index), Sense::click());

		if level.enabled {
			if click.hovered() {
				ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
			}

			if click.clicked() && !is_chosen {
				picked = Some(level.value);
			}
		}

		ui.add_space(6.0);
	}

	paint_track(ui, &stops, levels, chosen, palette.accent, palette.border);

	picked
}

fn paint_track<T>(
	ui: &Ui,
	stops: &[egui::Pos2],
	levels: &[Level<T>],
	chosen: Option<usize>,
	accent: Color32,
	idle: Color32,
) {
	let (Some(first), Some(last)) = (stops.first(), stops.last()) else {
		return;
	};
	let painter = ui.painter();

	painter.line_segment([*first, *last], Stroke::new(2.0, idle));

	if let Some(chosen) = chosen {
		painter.line_segment([*first, stops[chosen]], Stroke::new(2.0, accent));
	}

	let background = ui.visuals().panel_fill;

	for (index, (stop, level)) in stops.iter().zip(levels).enumerate() {
		let reached = chosen.is_some_and(|chosen| index <= chosen);

		if chosen == Some(index) {
			painter.circle(*stop, CHOSEN_RADIUS, accent, Stroke::new(2.0, background));
		} else if reached {
			painter.circle_filled(*stop, STOP_RADIUS, accent);
		} else {
			let edge = if level.enabled {
				idle
			} else {
				idle.gamma_multiply(0.5)
			};

			painter.circle(*stop, STOP_RADIUS, background, Stroke::new(2.0, edge));
		}
	}
}
