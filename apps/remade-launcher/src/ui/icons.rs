//! Icons from the Phosphor font, by codepoint.
//!
//! The font ships in `assets/fonts` and is registered as a fallback for the
//! interface font, so these characters draw as icons in any text. Only the
//! ones the interface uses are listed. A glyph is not a label: anything that
//! shows one must also have text or an accessible name.

pub(crate) const COMPASS: &str = "\u{E1C8}";
pub(crate) const BOOKS: &str = "\u{E758}";
pub(crate) const DOWNLOAD: &str = "\u{E20C}";
pub(crate) const GITHUB: &str = "\u{E576}";
pub(crate) const GEAR: &str = "\u{E270}";
pub(crate) const SEARCH: &str = "\u{E30C}";
pub(crate) const X: &str = "\u{E4F6}";
pub(crate) const WARNING: &str = "\u{E4E0}";
pub(crate) const SEAL_CHECK: &str = "\u{E606}";
pub(crate) const SEAL_WARNING: &str = "\u{E60C}";
pub(crate) const GRID: &str = "\u{E464}";
pub(crate) const LIST: &str = "\u{E2F0}";
pub(crate) const SPARKLE: &str = "\u{E6A2}";
pub(crate) const STAR: &str = "\u{E46A}";
pub(crate) const TRASH: &str = "\u{E4A6}";
pub(crate) const PLAY: &str = "\u{E3D0}";
pub(crate) const ARROW_CLOCKWISE: &str = "\u{E036}";
pub(crate) const ARROW_RIGHT: &str = "\u{E06C}";
pub(crate) const WIFI_SLASH: &str = "\u{E4F2}";
pub(crate) const WARNING_CIRCLE: &str = "\u{E4E2}";
pub(crate) const APPLE: &str = "\u{E516}";
pub(crate) const WINDOWS: &str = "\u{E692}";
pub(crate) const LINUX: &str = "\u{EB02}";
pub(crate) const TERMINAL: &str = "\u{E47E}";
pub(crate) const NOTE_PENCIL: &str = "\u{E34C}";
pub(crate) const TERMINAL_WINDOW: &str = "\u{EAE8}";
pub(crate) const MUSIC_NOTES: &str = "\u{E340}";
pub(crate) const PAINT_BRUSH: &str = "\u{E6F0}";
pub(crate) const GAUGE: &str = "\u{E628}";
pub(crate) const HARD_DRIVES: &str = "\u{E2A0}";
pub(crate) const NEWSPAPER: &str = "\u{E344}";
pub(crate) const IMAGE: &str = "\u{E2CA}";
pub(crate) const PENCIL_LINE: &str = "\u{EBC6}";
pub(crate) const ARROW_LEFT: &str = "\u{E058}";
pub(crate) const SIDEBAR: &str = "\u{EC24}";

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn every_icon_is_a_single_private_use_character() {
		for icon in [
			COMPASS,
			BOOKS,
			DOWNLOAD,
			GITHUB,
			GEAR,
			SEARCH,
			X,
			ARROW_RIGHT,
			SIDEBAR,
			WARNING,
		] {
			let mut characters = icon.chars();
			let first = characters.next().unwrap();

			assert!(characters.next().is_none(), "{icon:?}");
			assert!(
				('\u{E000}'..='\u{F8FF}').contains(&first),
				"{icon:?} is outside the private use area"
			);
		}
	}
}
