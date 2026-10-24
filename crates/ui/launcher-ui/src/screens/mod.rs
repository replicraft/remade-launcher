//! The screens. Each one is a plain function or small struct over `egui::Ui`.

mod add;
mod explore;
pub(crate) mod library;
mod navigation;
mod settings;
pub(crate) mod sheet;

pub(crate) use add::AddView;
pub(crate) use explore::ExploreView;
pub(crate) use library::library;
pub(crate) use navigation::navigation;
pub(crate) use settings::settings;

use egui::{Frame, Margin, RichText, Ui};
use launcher_core::{CustomPreparation, InstalledApp};
use launcher_model::InstallationId;

use crate::theme;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Screen {
	Explore,
	Library,
	Add,
	Settings,
}

impl Screen {
	pub(crate) const ALL: [Self; 4] = [Self::Explore, Self::Library, Self::Add, Self::Settings];

	pub(crate) const fn title(self) -> &'static str {
		match self {
			Self::Explore => "Explore",
			Self::Library => "Library",
			Self::Add => "Add from GitHub",
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
	ui.label(RichText::new(headline).size(18.0).strong());
	ui.label(RichText::new(explanation).color(theme::muted(ui)));
}
