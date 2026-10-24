//! A rounded square standing in for an app's icon: its first letter on a
//! colour picked from its name, so every app looks the same every time.

use egui::{Align2, Color32, FontId, Sense, Ui, vec2};

use crate::ui::theme;

/// Muted hues that read well on dark and light surfaces alike.
const HUES: [(u8, u8, u8); 8] = [
	(0x4F, 0x7C, 0xD9),
	(0x3E, 0x9B, 0x8C),
	(0x8A, 0x6B, 0xD1),
	(0xC4, 0x6F, 0x5A),
	(0xB8, 0x8A, 0x3A),
	(0x5A, 0x9E, 0x5A),
	(0xC2, 0x5F, 0x8E),
	(0x6B, 0x7A, 0x8C),
];

pub(crate) fn app_tile(ui: &mut Ui, name: &str, size: f32) {
	let (rect, _) = ui.allocate_exact_size(vec2(size, size), Sense::hover());
	let (red, green, blue) = HUES[hue_index(name)];
	let base = Color32::from_rgb(red, green, blue);
	let initial = name
		.chars()
		.find(|c| c.is_alphanumeric())
		.map_or('?', |c| c.to_ascii_uppercase());

	ui.painter()
		.rect_filled(rect, size * 0.24, base.gamma_multiply(0.30));
	ui.painter().text(
		rect.center(),
		Align2::CENTER_CENTER,
		initial,
		FontId::new(size * 0.46, theme::semibold()),
		base,
	);
}

/// A stable pick from the name, so a tile never changes colour.
fn hue_index(name: &str) -> usize {
	let sum = name.bytes().fold(5381_u32, |hash, byte| {
		hash.wrapping_mul(33).wrapping_add(u32::from(byte))
	});

	(sum as usize) % HUES.len()
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn a_name_always_gets_the_same_colour() {
		assert_eq!(hue_index("Lumen Notes"), hue_index("Lumen Notes"));
		assert!(hue_index("Quill") < HUES.len());
	}
}
