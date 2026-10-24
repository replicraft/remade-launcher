use egui::{ComboBox, RichText, Ui};
use launcher_background::control::BackgroundStatus;
use launcher_background::registration::RegistrationState;
use launcher_core::UpdateSettings;
use launcher_model::{MinimumAge, ThemePreference, UpdateMode, product};

use super::page;
use crate::ui::components::danger_button;
use crate::ui::{icons, theme};

pub(crate) enum SettingsAction {
	None,
	Theme(ThemePreference),
	CopyDiagnostics,
	/// The person picked an update mode. Automatic ones ask first.
	PickMode(UpdateMode),
	SetMinimumAge(MinimumAge),
	RestoreDefaults,
	StopBackground,
}

pub(crate) struct SettingsView<'a> {
	pub(crate) theme: ThemePreference,
	pub(crate) updates: UpdateSettings,
	pub(crate) background: Option<&'a BackgroundStatus>,
	pub(crate) logo: &'a egui::TextureHandle,
	/// Scroll the About section into view.
	pub(crate) show_about: bool,
}

pub(crate) fn settings(ui: &mut Ui, view: SettingsView<'_>) -> SettingsAction {
	let mut action = SettingsAction::None;

	page(ui, "Settings", |ui| {
		egui::ScrollArea::vertical()
			.auto_shrink([false, false])
			.show(ui, |ui| {
				appearance(ui, view.theme, &mut action);
				ui.add_space(16.0);
				updates(ui, &view.updates, view.background, &mut action);
				ui.add_space(16.0);
				support(ui, &mut action);
				ui.add_space(16.0);
				about(ui, view.logo, view.show_about, &mut action);
			});
	});

	action
}

fn appearance(ui: &mut Ui, current: ThemePreference, action: &mut SettingsAction) {
	ui.label(
		RichText::new("Appearance")
			.size(18.0)
			.family(theme::semibold()),
	);

	ui.horizontal(|ui| {
		for theme in ThemePreference::ALL {
			if ui.radio(current == theme, theme.label()).clicked() {
				*action = SettingsAction::Theme(theme);
			}
		}
	});
}

fn updates(
	ui: &mut Ui,
	settings: &UpdateSettings,
	background: Option<&BackgroundStatus>,
	action: &mut SettingsAction,
) {
	ui.label(
		RichText::new("Updates")
			.size(18.0)
			.family(theme::semibold()),
	);
	ui.label(
		RichText::new(
			"How installed apps are updated. This does not affect updates to the Launcher itself.",
		)
		.color(theme::muted(ui)),
	);
	ui.add_space(4.0);

	// One of three, never three independent switches.
	for mode in UpdateMode::ALL {
		ui.horizontal_top(|ui| {
			if ui.radio(settings.mode == mode, "").clicked() && settings.mode != mode {
				*action = SettingsAction::PickMode(mode);
			}

			ui.vertical(|ui| {
				let title = RichText::new(mode.label()).family(theme::semibold());
				let title = if mode == UpdateMode::Daemon {
					title.color(ui.visuals().warn_fg_color)
				} else {
					title
				};

				ui.label(title);
				ui.label(
					RichText::new(mode.description())
						.small()
						.color(theme::muted(ui)),
				);
			});
		});

		ui.add_space(2.0);
	}

	ui.add_space(8.0);
	minimum_age(ui, settings, action);

	if settings.mode == UpdateMode::Daemon
		|| background.is_some_and(|status| status.running.is_some())
	{
		ui.add_space(8.0);
		background_status(ui, background, action);
	}
}

fn minimum_age(ui: &mut Ui, settings: &UpdateSettings, action: &mut SettingsAction) {
	ui.label(RichText::new("Auto-update Default Minimum Release Age").family(theme::semibold()));
	ui.label(
		RichText::new(if settings.mode.is_automatic() {
			"A release must be at least this old before an automatic update installs it. Each app can use its own."
		} else {
			"Used only by the automatic modes. It never holds back an update you start yourself."
		})
		.small()
		.color(theme::muted(ui)),
	);

	// Only the presets: a free-form duration is one more thing to get wrong.
	let current = MinimumAge::PRESETS
		.iter()
		.find(|(_, age)| *age == settings.minimum_age)
		.map_or_else(
			|| settings.minimum_age.short(),
			|(label, _)| (*label).to_owned(),
		);

	ComboBox::from_id_salt("minimum-age")
		.selected_text(current)
		.show_ui(ui, |ui| {
			for (label, age) in MinimumAge::PRESETS {
				if ui
					.selectable_label(settings.minimum_age == age, label)
					.clicked()
				{
					*action = SettingsAction::SetMinimumAge(age);
				}
			}
		});

	if settings.minimum_age == MinimumAge::NONE {
		ui.colored_label(
			ui.visuals().warn_fg_color,
			"No delay installs a release the moment it is published. That is riskier.",
		);
	}
}

/// The updater's real state. It is never "Active" just because the setting
/// is on.
fn background_status(
	ui: &mut Ui,
	background: Option<&BackgroundStatus>,
	action: &mut SettingsAction,
) {
	ui.label(RichText::new("Background updater").family(theme::semibold()));

	let Some(status) = background else {
		ui.label(RichText::new("Checking…").color(theme::muted(ui)));

		return;
	};

	let (label, detail) = match (&status.running, &status.registration) {
		(Some(_), _) => ("Active", None),
		(None, RegistrationState::Registered) => {
			("Stopped", Some("It will start when you sign in."))
		}
		(None, RegistrationState::PendingApproval) => (
			"Pending OS approval",
			Some("Approve it in your system's login items."),
		),
		(None, RegistrationState::DisabledByOs) => (
			"Disabled by OS",
			Some("You turned it off in system settings. The Launcher will not turn it back on."),
		),
		(None, RegistrationState::NotInstalled) => ("Not installed", None),
		(None, RegistrationState::Failed(why)) => ("Failed", Some(why.as_str())),
		(None, RegistrationState::Unsupported(why)) => ("Unavailable here", Some(why.as_str())),
	};

	ui.label(label);

	if let Some(detail) = detail {
		ui.label(RichText::new(detail).small().color(theme::muted(ui)));
	}

	if let Some(info) = &status.running {
		if let Some(at) = info.last_completed.or(info.last_attempted) {
			ui.label(
				RichText::new(format!(
					"Last check: {}",
					crate::ui::screens::sheet::when(at)
				))
				.small()
				.color(theme::muted(ui)),
			);
		}

		if let Some(outcome) = &info.last_outcome {
			ui.label(
				RichText::new(format!("Last result: {outcome}"))
					.small()
					.color(theme::muted(ui)),
			);
		}

		if let Some(next) = info.next_due {
			ui.label(
				RichText::new(format!(
					"Next check: {}",
					crate::ui::screens::sheet::when(next)
				))
				.small()
				.color(theme::muted(ui)),
			);
		}

		if ui.button("Stop background updates").clicked() {
			*action = SettingsAction::StopBackground;
		}
	}
}

fn support(ui: &mut Ui, action: &mut SettingsAction) {
	ui.label(
		RichText::new("Support")
			.size(18.0)
			.family(theme::semibold()),
	);
	ui.add_space(4.0);

	if ui.button("Copy support report").clicked() {
		*action = SettingsAction::CopyDiagnostics;
	}

	ui.label(
		RichText::new(
			"The report lists versions and counts only. It has no file paths, links, or app names.",
		)
		.small()
		.color(theme::muted(ui)),
	);
}

fn about(ui: &mut Ui, logo: &egui::TextureHandle, scroll_to: bool, action: &mut SettingsAction) {
	let heading = ui.label(RichText::new("About").size(18.0).family(theme::semibold()));

	if scroll_to {
		heading.scroll_to_me(Some(egui::Align::TOP));
	}

	ui.add_space(8.0);

	ui.horizontal(|ui| {
		ui.image((logo.id(), egui::vec2(56.0, 56.0)));

		ui.vertical(|ui| {
			ui.spacing_mut().item_spacing.y = 2.0;
			ui.label(
				RichText::new(format!("{} by Replicraft", product::NAME))
					.size(16.0)
					.family(theme::semibold()),
			);
			ui.label(
				RichText::new(format!("Version {}", product::VERSION)).color(theme::muted(ui)),
			);
			ui.label(
				RichText::new(product::TAGLINE)
					.small()
					.color(theme::muted(ui)),
			);
		});
	});

	ui.add_space(10.0);

	egui::Grid::new("about-facts")
		.num_columns(2)
		.spacing([16.0, 6.0])
		.show(ui, |ui| {
			fact(
				ui,
				"Platform",
				&format!("{} {}", std::env::consts::OS, std::env::consts::ARCH),
			);
			fact(ui, "License", env!("CARGO_PKG_LICENSE"));

			ui.label(RichText::new("Website").color(theme::muted(ui)));
			ui.hyperlink_to("remade.wiki/launcher", env!("CARGO_PKG_HOMEPAGE"));
			ui.end_row();
		});

	ui.add_space(8.0);

	ui.hyperlink_to(
		format!("{}  View the source on GitHub", icons::GITHUB),
		env!("CARGO_PKG_REPOSITORY"),
	);

	ui.add_space(8.0);

	ui.collapsing("Open source notices", |ui| {
		for (name, license, note) in NOTICES {
			ui.horizontal_wrapped(|ui| {
				ui.label(RichText::new(*name).family(theme::semibold()));
				ui.label(RichText::new(*license).color(theme::muted(ui)));
				ui.label(RichText::new(*note).small().color(theme::muted(ui)));
			});
		}

		ui.label(
			RichText::new("The full list of Rust libraries and their licenses is in the project's lock file and source repository.")
				.small()
				.color(theme::muted(ui)),
		);
	});

	ui.add_space(12.0);

	// The consequences are spelled out in the confirmation that follows.
	if danger_button(ui, "Restore defaults").clicked() {
		*action = SettingsAction::RestoreDefaults;
	}
}

fn fact(ui: &mut Ui, label: &str, value: &str) {
	ui.label(RichText::new(label).color(theme::muted(ui)));
	ui.label(value);
	ui.end_row();
}

/// Bundled assets, named here so they are credited in the app itself.
const NOTICES: &[(&str, &str, &str)] = &[
	("Open Sans", "SIL Open Font License 1.1", "Interface font"),
	("Phosphor Icons", "MIT License", "Icon font"),
	("egui", "MIT or Apache-2.0", "Interface toolkit"),
];
