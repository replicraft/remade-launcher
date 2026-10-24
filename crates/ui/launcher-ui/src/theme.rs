//! The Remade look: neutral surfaces, one restrained blue for focus and
//! selection, and Open Sans.
//!
//! There is no brand accent color in the interface chrome. Selection is a
//! quiet gray fill, and blue appears only where something needs attention or
//! has the keyboard focus. The Remade mark keeps its own colors.

use std::sync::Arc;

use egui::{
	Color32, Context, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Stroke,
	TextStyle, Theme, Visuals,
};
use launcher_model::ThemePreference;

pub(crate) const SIDEBAR_WIDTH: f32 = 244.0;
pub(crate) const CONTENT_MARGIN: i8 = 28;

pub(crate) struct Palette {
	pub(crate) background: Color32,
	pub(crate) sidebar: Color32,
	pub(crate) surface: Color32,
	pub(crate) surface_hover: Color32,
	pub(crate) surface_active: Color32,
	pub(crate) selected: Color32,
	pub(crate) border: Color32,
	pub(crate) text: Color32,
	pub(crate) text_muted: Color32,
	/// Focus rings, the update badge, and links.
	pub(crate) accent: Color32,
	pub(crate) danger: Color32,
}

pub(crate) const DARK: Palette = Palette {
	background: Color32::from_rgb(0x0F, 0x0F, 0x10),
	sidebar: Color32::from_rgb(0x0C, 0x0C, 0x0D),
	surface: Color32::from_rgb(0x17, 0x17, 0x19),
	surface_hover: Color32::from_rgb(0x1F, 0x1F, 0x22),
	surface_active: Color32::from_rgb(0x28, 0x28, 0x2C),
	selected: Color32::from_rgb(0x22, 0x22, 0x25),
	border: Color32::from_rgb(0x2A, 0x2A, 0x2E),
	text: Color32::from_rgb(0xEC, 0xEC, 0xEE),
	text_muted: Color32::from_rgb(0x95, 0x95, 0x9C),
	accent: Color32::from_rgb(0x5B, 0x95, 0xF2),
	danger: Color32::from_rgb(0xE5, 0x48, 0x4D),
};

pub(crate) const LIGHT: Palette = Palette {
	background: Color32::from_rgb(0xF7, 0xF7, 0xF8),
	sidebar: Color32::from_rgb(0xEF, 0xEF, 0xF1),
	surface: Color32::from_rgb(0xFF, 0xFF, 0xFF),
	surface_hover: Color32::from_rgb(0xF0, 0xF0, 0xF2),
	surface_active: Color32::from_rgb(0xE6, 0xE6, 0xEA),
	selected: Color32::from_rgb(0xE2, 0xE2, 0xE7),
	border: Color32::from_rgb(0xDA, 0xDA, 0xDF),
	text: Color32::from_rgb(0x16, 0x16, 0x18),
	text_muted: Color32::from_rgb(0x56, 0x56, 0x5E),
	accent: Color32::from_rgb(0x2B, 0x6C, 0xDB),
	danger: Color32::from_rgb(0xD1, 0x2F, 0x34),
};

/// The palette for the theme `ui` is drawn in.
pub(crate) fn palette(ui: &egui::Ui) -> &'static Palette {
	if ui.visuals().dark_mode {
		&DARK
	} else {
		&LIGHT
	}
}

/// The palette for the theme `context` is drawn in, for code outside a `Ui`.
pub(crate) fn palette_for(context: &egui::Context) -> &'static Palette {
	if context.global_style().visuals.dark_mode {
		&DARK
	} else {
		&LIGHT
	}
}

/// A semi-bold family, for headings and emphasis. egui cannot make text bold
/// by itself, so emphasis uses the real semi-bold font.
pub(crate) fn semibold() -> FontFamily {
	FontFamily::Name("semibold".into())
}

fn fonts() -> FontDefinitions {
	let mut fonts = FontDefinitions::default();

	let data = |bytes: &'static [u8]| Arc::new(FontData::from_static(bytes));

	fonts.font_data.insert(
		"open-sans".into(),
		data(include_bytes!(
			"../../../../assets/fonts/OpenSans-Regular.ttf"
		)),
	);
	fonts.font_data.insert(
		"open-sans-semibold".into(),
		data(include_bytes!(
			"../../../../assets/fonts/OpenSans-SemiBold.ttf"
		)),
	);
	fonts.font_data.insert(
		"phosphor".into(),
		data(include_bytes!("../../../../assets/fonts/Phosphor.ttf")),
	);

	// Open Sans first, then the icon font, then whatever egui ships as
	// fallbacks for scripts Open Sans does not cover.
	let proportional = fonts.families.entry(FontFamily::Proportional).or_default();

	proportional.insert(0, "open-sans".into());
	proportional.insert(1, "phosphor".into());

	let mut semi = vec!["open-sans-semibold".to_owned(), "phosphor".to_owned()];

	semi.extend(
		fonts
			.families
			.get(&FontFamily::Proportional)
			.into_iter()
			.flatten()
			.filter(|name| !name.starts_with("open-sans") && *name != "phosphor")
			.cloned(),
	);
	fonts.families.insert(semibold(), semi);

	fonts
}

/// Registers fonts, both themes, and the type scale. Call once.
pub(crate) fn install(context: &Context) {
	context.set_fonts(fonts());

	context.set_visuals_of(Theme::Dark, visuals(&DARK, Visuals::dark()));
	context.set_visuals_of(Theme::Light, visuals(&LIGHT, Visuals::light()));

	context.all_styles_mut(|style| {
		style.spacing.item_spacing = egui::vec2(10.0, 10.0);
		style.spacing.button_padding = egui::vec2(14.0, 8.0);
		style.spacing.interact_size.y = 34.0;

		style.text_styles = [
			(TextStyle::Heading, FontId::new(25.0, semibold())),
			(TextStyle::Body, FontId::new(14.5, FontFamily::Proportional)),
			(
				TextStyle::Button,
				FontId::new(14.5, FontFamily::Proportional),
			),
			(
				TextStyle::Small,
				FontId::new(12.5, FontFamily::Proportional),
			),
			(
				TextStyle::Monospace,
				FontId::new(13.5, FontFamily::Monospace),
			),
		]
		.into();
	});
}

/// Applies the user's choice. "System" follows the operating system.
pub(crate) fn select(context: &Context, preference: ThemePreference) {
	context.set_theme(match preference {
		ThemePreference::System => egui::ThemePreference::System,
		ThemePreference::Light => egui::ThemePreference::Light,
		ThemePreference::Dark => egui::ThemePreference::Dark,
	});
}

/// The muted text color for the active theme.
pub(crate) fn muted(ui: &egui::Ui) -> Color32 {
	palette(ui).text_muted
}

fn visuals(palette: &Palette, mut base: Visuals) -> Visuals {
	let rounding = CornerRadius::same(8);

	base.override_text_color = None;
	base.panel_fill = palette.background;
	base.window_fill = palette.surface;
	base.extreme_bg_color = palette.surface;
	base.faint_bg_color = palette.surface;
	base.hyperlink_color = palette.accent;
	base.window_corner_radius = CornerRadius::same(12);
	base.error_fg_color = palette.danger;

	// Selected text and selectable widgets: a calm tint of the accent, with
	// the normal text color on top so it stays readable in both themes.
	base.selection.bg_fill = palette.accent.gamma_multiply(0.35);
	base.selection.stroke = Stroke::new(1.0, palette.text);

	let widgets = &mut base.widgets;

	widgets.noninteractive.bg_fill = palette.background;
	widgets.noninteractive.bg_stroke = Stroke::new(1.0, palette.border);
	widgets.noninteractive.fg_stroke = Stroke::new(1.0, palette.text);
	widgets.noninteractive.corner_radius = rounding;

	for (state, fill) in [
		(&mut widgets.inactive, palette.surface),
		(&mut widgets.hovered, palette.surface_hover),
		(&mut widgets.active, palette.surface_active),
		(&mut widgets.open, palette.surface_hover),
	] {
		state.bg_fill = fill;
		state.weak_bg_fill = fill;
		state.bg_stroke = Stroke::new(1.0, palette.border);
		state.fg_stroke = Stroke::new(1.0, palette.text);
		state.corner_radius = rounding;
	}

	// The keyboard focus ring must stand out in both themes.
	widgets.hovered.bg_stroke = Stroke::new(1.0, palette.text_muted);
	widgets.active.bg_stroke = Stroke::new(1.5, palette.accent);

	base
}

#[cfg(test)]
mod tests {
	use super::*;

	/// WCAG relative luminance contrast ratio between two colors.
	fn contrast(a: Color32, b: Color32) -> f64 {
		let luminance = |color: Color32| {
			let channel = |value: u8| {
				let value = f64::from(value) / 255.0;

				if value <= 0.03928 {
					value / 12.92
				} else {
					((value + 0.055) / 1.055).powf(2.4)
				}
			};

			0.2126 * channel(color.r()) + 0.7152 * channel(color.g()) + 0.0722 * channel(color.b())
		};

		let (high, low) = (
			luminance(a).max(luminance(b)),
			luminance(a).min(luminance(b)),
		);

		(high + 0.05) / (low + 0.05)
	}

	#[test]
	fn text_is_readable_on_every_surface_in_both_themes() {
		for (name, palette) in [("dark", &DARK), ("light", &LIGHT)] {
			for surface in [
				palette.background,
				palette.sidebar,
				palette.surface,
				palette.selected,
			] {
				assert!(contrast(palette.text, surface) >= 7.0, "{name}: body text");
				assert!(
					contrast(palette.text_muted, surface) >= 4.5,
					"{name}: muted text"
				);
			}

			assert!(
				contrast(palette.accent, palette.background) >= 3.0,
				"{name}: the focus ring against the page"
			);
			assert!(
				contrast(palette.danger, palette.surface) >= 3.0,
				"{name}: the danger color"
			);
		}
	}

	#[test]
	fn the_fonts_register_with_icons_as_a_fallback() {
		let definitions = fonts();
		let proportional = &definitions.families[&FontFamily::Proportional];

		assert_eq!(proportional[0], "open-sans");
		assert!(proportional.contains(&"phosphor".to_owned()));
		assert_eq!(definitions.families[&semibold()][0], "open-sans-semibold");
	}
}
