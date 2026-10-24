//! The screens. Each one is a plain function or small struct over `egui::Ui`.

pub(crate) mod add;
mod explore;
pub(crate) mod library;
pub(crate) mod navigation;
mod settings;
pub(crate) mod sheet;
pub(crate) mod updates;

pub(crate) use add::AddView;
pub(crate) use explore::ExploreView;
pub(crate) use library::{LibraryView, library};
pub(crate) use navigation::navigation;
pub(crate) use settings::{SettingsAction, SettingsView, settings};
pub(crate) use updates::{UpdatesView, updates};

use egui::{Frame, Margin, RichText, Ui};
use launcher_core::{CustomPreparation, InstalledApp};
use launcher_model::InstallationId;

use crate::ui::theme;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Screen {
	Explore,
	Library,
	Updates,
	Settings,
}

impl Screen {
	pub(crate) const fn title(self) -> &'static str {
		match self {
			Self::Explore => "Explore",
			Self::Library => "Library",
			Self::Updates => "Updates",
			Self::Settings => "Settings",
		}
	}
}

/// What a screen asks the application to do. Screens never call the engine
/// themselves.
#[derive(Debug)]
pub(crate) enum Action {
	Install {
		slug: String,
	},
	Launch(InstallationId),
	Update(InstallationId),
	Uninstall(InstalledApp),
	LookUp(String),
	ChooseFile {
		preparation: Box<CustomPreparation>,
		asset_id: u64,
	},
	SetPolicy(InstallationId, launcher_storage::AppPolicy),
	Pin(InstallationId, bool),
	/// A read-only look for updates to everything installed.
	CheckUpdates,
	UpdateAll,
	/// Stop the running install or update at the next safe point.
	Cancel,
	ReleaseNotes {
		slug: String,
		tag: String,
		app: String,
	},
}

/// A screen title with consistent margins.
fn page(ui: &mut Ui, title: &str, contents: impl FnOnce(&mut Ui)) {
	Frame::new()
		.inner_margin(Margin::same(theme::CONTENT_MARGIN))
		.show(ui, |ui| {
			ui.heading(title);
			ui.add_space(8.0);

			contents(ui);
		});
}

/// An honest empty state: it says what is missing and why, and never
/// pretends something is loading.
fn unavailable(ui: &mut Ui, headline: &str, explanation: &str) {
	ui.add_space(24.0);
	ui.label(
		RichText::new(headline)
			.size(18.0)
			.family(crate::ui::theme::semibold()),
	);
	ui.label(RichText::new(explanation).color(theme::muted(ui)));
}
