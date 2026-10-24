//! Fonts for scripts the bundled font does not cover.
//!
//! Open Sans covers the Latin languages. Japanese, Chinese, and Hindi need
//! fonts with their characters, and shipping those would add many megabytes,
//! so the launcher borrows the ones the system already has. If none is found
//! the text still shows, as empty boxes for the missing characters.

use std::path::Path;

use egui::FontData;
use launcher_model::Language;

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
		Language::Hindi => vec![
			candidate("/System/Library/Fonts/Kohinoor.ttc", 0),
			candidate(
				"/System/Library/Fonts/Supplemental/Devanagari Sangam MN.ttc",
				0,
			),
			candidate(r"C:\Windows\Fonts\Nirmala.ttc", 0),
			candidate(r"C:\Windows\Fonts\mangal.ttf", 0),
			candidate(
				"/usr/share/fonts/truetype/noto/NotoSansDevanagari-Regular.ttf",
				0,
			),
			candidate(
				"/usr/share/fonts/opentype/noto/NotoSansDevanagari-Regular.ttf",
				0,
			),
			candidate("/usr/share/fonts/noto/NotoSansDevanagari-Regular.ttf", 0),
			candidate(
				"/usr/share/fonts/truetype/lohit-devanagari/Lohit-Devanagari.ttf",
				0,
			),
			candidate("/usr/share/fonts/truetype/freefont/FreeSans.ttf", 0),
		],
		Language::English | Language::German | Language::PortugueseBrazil | Language::Auto => {
			Vec::new()
		}
	}
}

/// Fonts to add after the bundled ones, as `(name, data)`. At most two are
/// loaded: the first usually has everything, and a second covers its gaps.
pub(crate) fn system_fallbacks(language: Language) -> Vec<(String, FontData)> {
	const MOST: usize = 2;

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

/// Whether two languages need the same extra fonts, so switching between
/// them does not reload anything.
pub(crate) fn same_fonts(a: Language, b: Language) -> bool {
	let group = |language| match language {
		Language::Japanese => 1,
		Language::ChineseSimplified => 2,
		Language::Hindi => 3,
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
		for language in [
			Language::Japanese,
			Language::ChineseSimplified,
			Language::Hindi,
		] {
			assert!(candidates(language).len() >= 6, "{language:?}");
		}

		assert!(same_fonts(Language::English, Language::German));
		assert!(!same_fonts(Language::Japanese, Language::ChineseSimplified));
	}
}
