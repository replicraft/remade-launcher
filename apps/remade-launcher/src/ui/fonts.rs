//! Fonts for scripts Open Sans does not cover.
//!
//! Open Sans covers the Latin languages. Japanese, Chinese, and Hindi use
//! Noto Sans, the same humanist design, bundled as small subsets (see
//! `assets/fonts/make-subsets.py`): every character the interface uses, plus
//! the common kanji and hanzi. All three are always loaded, so each language
//! can be shown by its own name in the language list whatever the interface
//! says now. Anything rarer, such as an unusual character in an app's name,
//! falls back to a font the system has.

use std::path::Path;

use egui::FontData;
use launcher_model::Language;

/// A bundled script: the key used in font names, and its two weights.
pub(crate) struct Bundled {
	pub(crate) key: &'static str,
	pub(crate) regular: &'static [u8],
	pub(crate) semibold: &'static [u8],
}

const JAPANESE: Bundled = Bundled {
	key: "jp",
	regular: include_bytes!("../../../../assets/fonts/NotoSansJP-Subset-Regular.ttf"),
	semibold: include_bytes!("../../../../assets/fonts/NotoSansJP-Subset-SemiBold.ttf"),
};

const CHINESE: Bundled = Bundled {
	key: "sc",
	regular: include_bytes!("../../../../assets/fonts/NotoSansSC-Subset-Regular.ttf"),
	semibold: include_bytes!("../../../../assets/fonts/NotoSansSC-Subset-SemiBold.ttf"),
};

const DEVANAGARI: Bundled = Bundled {
	key: "deva",
	regular: include_bytes!("../../../../assets/fonts/NotoSansDevanagari-Regular.ttf"),
	semibold: include_bytes!("../../../../assets/fonts/NotoSansDevanagari-SemiBold.ttf"),
};

/// The bundled scripts in the order they are tried. Japanese and Chinese
/// share many characters drawn differently, so the language in use goes
/// first.
pub(crate) fn bundled(language: Language) -> [Bundled; 3] {
	match language {
		Language::ChineseSimplified => [CHINESE, JAPANESE, DEVANAGARI],
		_ => [JAPANESE, CHINESE, DEVANAGARI],
	}
}

/// A font file and which face of it to use.
struct Candidate {
	path: &'static str,
	face: u32,
}

const fn candidate(path: &'static str, face: u32) -> Candidate {
	Candidate { path, face }
}

fn candidates(language: Language) -> Vec<Candidate> {
	match language {
		Language::Japanese => vec![
			// macOS
			candidate("/System/Library/Fonts/ヒラギノ角ゴシック W3.ttc", 0),
			candidate("/System/Library/Fonts/Hiragino Sans GB.ttc", 0),
			candidate("/System/Library/Fonts/PingFang.ttc", 0),
			// Windows
			candidate(r"C:\Windows\Fonts\YuGothR.ttc", 0),
			candidate(r"C:\Windows\Fonts\meiryo.ttc", 0),
			candidate(r"C:\Windows\Fonts\msgothic.ttc", 0),
			// Linux. Noto CJK holds Japanese as its first face.
			candidate("/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc", 0),
			candidate("/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc", 0),
			candidate(
				"/usr/share/fonts/google-noto-cjk/NotoSansCJK-Regular.ttc",
				0,
			),
			candidate("/usr/share/fonts/truetype/noto/NotoSansCJK-Regular.ttc", 0),
			candidate("/usr/share/fonts/truetype/fonts-japanese-gothic.ttf", 0),
		],
		Language::ChineseSimplified => vec![
			candidate("/System/Library/Fonts/PingFang.ttc", 0),
			candidate("/System/Library/Fonts/Hiragino Sans GB.ttc", 0),
			candidate("/System/Library/Fonts/STHeiti Light.ttc", 0),
			candidate(r"C:\Windows\Fonts\msyh.ttc", 0),
			candidate(r"C:\Windows\Fonts\simsun.ttc", 0),
			// Noto CJK holds Simplified Chinese as its third face.
			candidate("/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc", 2),
			candidate("/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc", 2),
			candidate(
				"/usr/share/fonts/google-noto-cjk/NotoSansCJK-Regular.ttc",
				2,
			),
			candidate("/usr/share/fonts/truetype/noto/NotoSansCJK-Regular.ttc", 2),
			candidate("/usr/share/fonts/truetype/wqy/wqy-microhei.ttc", 0),
			candidate("/usr/share/fonts/truetype/wqy/wqy-zenhei.ttc", 0),
		],
		// Noto Sans Devanagari is bundled whole, so Hindi needs no more.
		Language::Hindi
		| Language::English
		| Language::Spanish
		| Language::German
		| Language::PortugueseBrazil
		| Language::Auto => Vec::new(),
	}
}

/// A system font to add after the bundled ones, for the rare characters they
/// leave out, such as an unusual kanji in an app's name. One is enough, and
/// these files are large, so only the first that exists is read.
pub(crate) fn system_fallbacks(language: Language) -> Vec<(String, FontData)> {
	const MOST: usize = 1;

	let mut found = Vec::new();

	for candidate in candidates(language) {
		if found.len() == MOST {
			break;
		}

		if !Path::new(candidate.path).is_file() {
			continue;
		}

		match std::fs::read(candidate.path) {
			Ok(bytes) => {
				let mut data = FontData::from_owned(bytes);

				data.index = candidate.face;
				found.push((format!("system-{}", found.len()), data));
			}
			Err(error) => {
				tracing::debug!(%error, path = candidate.path, "a system font could not be read");
			}
		}
	}

	if found.is_empty() && !candidates(language).is_empty() {
		tracing::warn!(?language, "no system font for this language was found");
	}

	found
}

/// Whether two languages load the same fonts in the same order, so switching
/// between them does not reload anything.
pub(crate) fn same_fonts(a: Language, b: Language) -> bool {
	let group = |language| match language {
		Language::Japanese => 1,
		Language::ChineseSimplified => 2,
		_ => 0,
	};

	group(a) == group(b)
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn latin_languages_need_nothing_extra() {
		for language in [
			Language::English,
			Language::German,
			Language::PortugueseBrazil,
		] {
			assert!(system_fallbacks(language).is_empty());
		}
	}

	#[test]
	fn the_scripts_that_need_a_font_have_places_to_look() {
		for language in [Language::Japanese, Language::ChineseSimplified] {
			assert!(candidates(language).len() >= 6, "{language:?}");
		}

		assert!(same_fonts(Language::English, Language::German));
		assert!(!same_fonts(Language::Japanese, Language::ChineseSimplified));
	}

	/// The characters of `text` that `font` has no glyph for.
	fn missing(font: &[u8], text: &str, wanted: impl Fn(char) -> bool) -> Vec<char> {
		use skrifa::{FontRef, MetadataProvider};

		let font = FontRef::new(font).expect("a bundled font is a font");
		let charmap = font.charmap();
		let mut missing: Vec<char> = text
			.chars()
			.filter(|c| wanted(*c) && charmap.map(*c).is_none())
			.collect();

		missing.sort_unstable();
		missing.dedup();
		missing
	}

	fn translations(folder: &str) -> String {
		let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
			.join("locales")
			.join(folder);

		std::fs::read_dir(root)
			.expect("the locale folder exists")
			.map(|entry| std::fs::read_to_string(entry.expect("a file").path()).expect("text"))
			.collect()
	}

	/// Kana, kanji, hanzi, and full-width forms.
	fn is_cjk(c: char) -> bool {
		u32::from(c) >= 0x2E80
	}

	#[test]
	fn the_bundled_fonts_cover_every_character_the_translations_use() {
		// Run `assets/fonts/make-subsets.py` again if this fails.
		for (folder, font, weight) in [
			("ja", JAPANESE.regular, "regular"),
			("ja", JAPANESE.semibold, "semibold"),
			("zh-CN", CHINESE.regular, "regular"),
			("zh-CN", CHINESE.semibold, "semibold"),
		] {
			assert_eq!(
				missing(font, &translations(folder), is_cjk),
				Vec::<char>::new(),
				"{folder} {weight}"
			);
		}

		let hindi = |c: char| ('\u{0900}'..='\u{097F}').contains(&c);

		for font in [DEVANAGARI.regular, DEVANAGARI.semibold] {
			assert_eq!(
				missing(font, &translations("hi"), hindi),
				Vec::<char>::new()
			);
		}
	}

	#[test]
	fn every_language_can_be_named_in_its_own_script() {
		for language in Language::TRANSLATED {
			let name = language
				.endonym()
				.expect("a translated language has a name");

			assert!(
				bundled(Language::English).iter().any(|script| {
					missing(script.regular, name, |c| {
						c as u32 > 0x2E7F || ('\u{0900}'..='\u{097F}').contains(&c)
					})
					.is_empty()
				}),
				"{name}"
			);
		}
	}
}
