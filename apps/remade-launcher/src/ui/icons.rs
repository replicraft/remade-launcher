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

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn every_icon_is_a_single_private_use_character() {
		for icon in [COMPASS, BOOKS, DOWNLOAD, GITHUB, GEAR, SEARCH, X] {
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
