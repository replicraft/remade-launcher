//! The Remade mark, decoded once from the packaged artwork.

use egui::{ColorImage, IconData};

/// The 256-pixel application icon. The file is the packaged brand export.
const ICON_PNG: &[u8] = include_bytes!("../../../../assets/icons/remade-launcher-256.png");

/// Decodes the icon into RGBA pixels.
fn decode() -> image::RgbaImage {
	image::load_from_memory_with_format(ICON_PNG, image::ImageFormat::Png)
		.expect("the packaged icon is a valid PNG")
		.into_rgba8()
}

pub(crate) fn window_icon() -> IconData {
	let pixels = decode();

	IconData {
		width: pixels.width(),
		height: pixels.height(),
		rgba: pixels.into_raw(),
	}
}

pub(crate) fn logo_image() -> ColorImage {
	let pixels = decode();
	let size = [pixels.width() as usize, pixels.height() as usize];

	ColorImage::from_rgba_unmultiplied(size, pixels.as_raw())
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn the_packaged_icon_decodes() {
		let icon = window_icon();

		assert_eq!((icon.width, icon.height), (256, 256));
		assert_eq!(icon.rgba.len(), 256 * 256 * 4);
	}
}
