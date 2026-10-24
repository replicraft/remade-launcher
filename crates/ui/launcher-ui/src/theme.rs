//! The Remade visual identity: a charcoal ground with a lime accent.
//!
//! Colors come from the brand mark. The lime is used sparingly, for the
//! selected state and primary actions, and never as body text on a light
//! background where it would fail contrast.

use egui::{Color32, Context, CornerRadius, FontFamily, FontId, Stroke, TextStyle, Theme, Visuals};
use launcher_model::ThemePreference;

pub(crate) const SIDEBAR_WIDTH: f32 = 220.0;
pub(crate) const CONTENT_MARGIN: i8 = 28;

struct Palette {
	background: Color32,
	surface: Color32,
	surface_hover: Color32,
	surface_active: Color32,
	border: Color32,
	text: Color32,
	text_muted: Color32,
	accent: Color32,
	accent_text: Color32,
}

const DARK: Palette = Palette {
	background: Color32::from_rgb(0x15, 0x19, 0x1F),
	surface: Color32::from_rgb(0x1D, 0x22, 0x2A),
	surface_hover: Color32::from_rgb(0x26, 0x2C, 0x36),
	surface_active: Color32::from_rgb(0x2F, 0x37, 0x43),
	border: Color32::from_rgb(0x32, 0x3A, 0x46),
	text: Color32::from_rgb(0xE9, 0xED, 0xF2),
	text_muted: Color32::from_rgb(0x9A, 0xA4, 0xB2),
	accent: Color32::from_rgb(0xC4, 0xF4, 0x77),
	accent_text: Color32::from_rgb(0x15, 0x19, 0x1F),
};

const LIGHT: Palette = Palette {
	background: Color32::from_rgb(0xF5, 0xF6, 0xF2),
	surface: Color32::from_rgb(0xFF, 0xFF, 0xFF),
	surface_hover: Color32::from_rgb(0xEC, 0xEE, 0xE6),
	surface_active: Color32::from_rgb(0xE0, 0xE4, 0xD8),
	border: Color32::from_rgb(0xD5, 0xD9, 0xCE),
	text: Color32::from_rgb(0x15, 0x19, 0x1F),
	text_muted: Color32::from_rgb(0x59, 0x62, 0x6E),
	accent: Color32::from_rgb(0x3F, 0x5F, 0x0F),
	accent_text: Color32::from_rgb(0xFF, 0xFF, 0xFF),
};

/// Registers both visual themes and the shared type scale. Call once.
pub(crate) fn install(context: &Context) {
	context.set_visuals_of(Theme::Dark, visuals(&DARK, Visuals::dark()));
	context.set_visuals_of(Theme::Light, visuals(&LIGHT, Visuals::light()));

	context.all_styles_mut(|style| {
		style.spacing.item_spacing = egui::vec2(10.0, 10.0);
		style.spacing.button_padding = egui::vec2(14.0, 8.0);
		style.spacing.interact_size.y = 34.0;

		style.text_styles = [
			(
				TextStyle::Heading,
				FontId::new(26.0, FontFamily::Proportional),
			),
			(TextStyle::Body, FontId::new(15.0, FontFamily::Proportional)),
			(
				TextStyle::Button,
				FontId::new(15.0, FontFamily::Proportional),
			),
			(
				TextStyle::Small,
				FontId::new(12.5, FontFamily::Proportional),
			),
			(
				TextStyle::Monospace,
				FontId::new(14.0, FontFamily::Monospace),
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
	if ui.visuals().dark_mode {
		DARK.text_muted
	} else {
		LIGHT.text_muted
	}
}

fn visuals(palette: &Palette, mut base: Visuals) -> Visuals {
	let rounding = CornerRadius::same(8);

	// No forced text color: the selected state must be able to draw dark
	// text on the lime fill, which a global override would defeat.
	base.override_text_color = None;
	base.panel_fill = palette.background;
	base.window_fill = palette.surface;
	base.extreme_bg_color = palette.surface;
	base.faint_bg_color = palette.surface;
	base.hyperlink_color = palette.accent;
	base.window_corner_radius = CornerRadius::same(12);

	base.selection.bg_fill = palette.accent;
	base.selection.stroke = Stroke::new(1.0, palette.accent_text);

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
	widgets.hovered.bg_stroke = Stroke::new(1.5, palette.accent);
	widgets.active.bg_stroke = Stroke::new(1.5, palette.accent);

	base
}
