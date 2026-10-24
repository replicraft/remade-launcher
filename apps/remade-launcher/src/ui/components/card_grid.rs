//! Lays cards out in even columns that fill the width.

use egui::{Layout, Ui, vec2};

const GAP: f32 = 14.0;

/// How many columns of at least `min_width` fit in `available`, and how wide
/// each card then is.
pub(crate) fn columns(available: f32, min_width: f32) -> (usize, f32) {
	let count = (((available + GAP) / (min_width + GAP)).floor() as usize).max(1);
	let width = (available - GAP * (count as f32 - 1.0)) / count as f32;

	(count, width.max(1.0))
}

/// Calls `card` once for each of `count` items, in rows, giving each a ui of
/// exactly `height` tall and an even share of the width.
pub(crate) fn card_grid(
	ui: &mut Ui,
	count: usize,
	min_width: f32,
	height: f32,
	mut card: impl FnMut(&mut Ui, usize),
) {
	let (per_row, width) = columns(ui.available_width(), min_width);

	for start in (0..count).step_by(per_row) {
		ui.horizontal_top(|ui| {
			ui.spacing_mut().item_spacing.x = GAP;

			for index in start..(start + per_row).min(count) {
				ui.allocate_ui_with_layout(
					vec2(width, height),
					Layout::top_down(egui::Align::Min),
					|ui| {
						ui.set_min_size(vec2(width, height));
						card(ui, index);
					},
				);
			}
		});

		ui.add_space(GAP);
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn narrow_windows_get_one_column() {
		assert_eq!(columns(200.0, 270.0).0, 1);
	}

	#[test]
	fn columns_fill_the_width_exactly() {
		let (count, width) = columns(900.0, 270.0);

		assert_eq!(count, 3);
		assert!((width * count as f32 + GAP * (count as f32 - 1.0) - 900.0).abs() < 0.01);
	}
}
