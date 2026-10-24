//! Settings: every choice the launcher offers, in groups.

mod about;
mod general;
mod launcher;
mod network;
mod notifications;
mod region;
mod storage;
mod support;
mod updates;

use std::path::PathBuf;

use egui::{ScrollArea, Sense, Ui};
use launcher_background::control::BackgroundStatus;
use launcher_core::{LauncherUpdateReport, RateLimit, StorageReport, TokenState, UpdateSettings};
use launcher_model::{MinimumAge, Preferences, ThemePreference, UpdateMode};

use self::network::NetworkView;
use super::page;
use crate::ui::components::panel;

pub(crate) enum SettingsAction {
	None,
	Theme(ThemePreference),
	/// Any change to the saved preferences. It carries the whole new set.
	Preferences(Box<Preferences>),
	CopyDiagnostics,
	/// The person picked an update mode. Automatic ones ask first.
	PickMode(UpdateMode),
	SetMinimumAge(MinimumAge),
	RestoreDefaults,
	StopBackground,
	SaveToken(String),
	ClearToken,
	CheckRateLimit,
	ShowFolder(PathBuf),
	ClearLeftovers,
	ClearCache,
	TestNotification,
	CheckLauncherUpdate,
	DownloadLauncherUpdate,
	RollBackLauncher,
}

/// Text being typed, which is not a saved setting until it is applied.
#[derive(Default)]
pub(crate) struct SettingsInputs {
	pub(crate) proxy_text: String,
	pub(crate) proxy_error: Option<launcher_model::preferences::ProxyProblem>,
	/// The custom-proxy fields are showing, even before one is saved.
	pub(crate) proxy_editing: bool,
	pub(crate) token_text: String,
}

pub(crate) struct SettingsView<'a> {
	pub(crate) theme: ThemePreference,
	pub(crate) updates: UpdateSettings,
	pub(crate) background: Option<&'a BackgroundStatus>,
	pub(crate) logo: &'a egui::TextureHandle,
	/// Scroll the About section into view.
	pub(crate) show_about: bool,
	pub(crate) prefs: &'a Preferences,
	pub(crate) inputs: &'a mut SettingsInputs,
	pub(crate) token: &'a TokenState,
	pub(crate) rate: Option<&'a Result<RateLimit, String>>,
	pub(crate) storage: Option<&'a StorageReport>,
	pub(crate) launcher: Option<&'a LauncherUpdateReport>,
	/// A look for a new launcher is under way.
	pub(crate) launcher_checking: bool,
}

pub(crate) fn settings(ui: &mut Ui, view: SettingsView<'_>) -> SettingsAction {
	let mut action = SettingsAction::None;
	let SettingsView {
		theme,
		updates,
		background,
		logo,
		show_about,
		prefs,
		inputs,
		token,
		rate,
		storage,
		launcher,
		launcher_checking,
	} = view;

	page(ui, &t!("settings-title"), |ui| {
		ScrollArea::vertical()
			.auto_shrink([false, false])
			.show(ui, |ui| {
				panel(ui, &t!("settings-appearance"), None, |ui| {
					general::general(ui, theme, prefs, &mut action)
				});
				ui.add_space(14.0);

				panel(ui, &t!("settings-region-panel"), None, |ui| {
					region::region(ui, prefs, &mut action)
				});
				ui.add_space(14.0);

				panel(
					ui,
					&t!("settings-updates-panel"),
					Some(&t!("settings-updates-panel.description")),
					|ui| updates::updates(ui, &updates, background, &mut action),
				);
				ui.add_space(14.0);

				panel(
					ui,
					&t!("settings-launcher-panel"),
					Some(&t!("settings-launcher-panel.description")),
					|ui| launcher::launcher(ui, prefs, launcher, launcher_checking, &mut action),
				);
				ui.add_space(14.0);

				panel(ui, &t!("settings-notifications-panel"), None, |ui| {
					notifications::notifications(ui, prefs, &mut action)
				});
				ui.add_space(14.0);

				panel(ui, &t!("settings-network-panel"), None, |ui| {
					network::network(ui, &NetworkView { prefs, token, rate }, inputs, &mut action);
				});
				ui.add_space(14.0);

				panel(ui, &t!("settings-storage-panel"), None, |ui| {
					storage::storage(ui, storage, &mut action)
				});
				ui.add_space(14.0);

				panel(ui, &t!("settings-support-panel"), None, |ui| {
					support::support(ui, &mut action)
				});
				ui.add_space(14.0);

				// An invisible marker the About menu item can scroll to.
				let anchor = ui.allocate_response(egui::vec2(0.0, 0.0), Sense::hover());

				if show_about {
					anchor.scroll_to_me(Some(egui::Align::TOP));
				}

				panel(ui, &t!("settings-about-panel"), None, |ui| {
					about::about(ui, logo, &mut action)
				});
				ui.add_space(24.0);
			});
	});

	action
}
