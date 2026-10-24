//! Add an app from a GitHub repository the user chooses.
//!
//! This path is separate from the catalog. Nothing here is reviewed by
//! Remade, and the screen says so before and after a lookup.

use egui::{RichText, ScrollArea, Ui};
use launcher_core::CustomPreparation;
use launcher_resolver::Fit;

use super::sheet::bytes;
use super::{Action, page};
use crate::theme;

#[derive(Default)]
pub(crate) struct AddView {
	url: String,
	lookup: Option<CustomPreparation>,
	pub(crate) loading: bool,
}

impl AddView {
	pub(crate) fn set_lookup(&mut self, lookup: CustomPreparation) {
		self.lookup = Some(lookup);
		self.loading = false;
	}

	pub(crate) fn lookup_failed(&mut self) {
		self.loading = false;
	}

	pub(crate) fn show(&mut self, ui: &mut Ui) -> Option<Action> {
		let mut action = None;

		page(ui, "Add from GitHub", |ui| {
			ui.label(
				RichText::new("Paste a public GitHub repository or release link. Remade has not reviewed apps you add this way, and a checksum cannot show who made a file or that it is safe.")
					.color(theme::muted(ui)),
			);

			ui.horizontal(|ui| {
				ui.add(
					egui::TextEdit::singleline(&mut self.url)
						.hint_text("https://github.com/owner/repository")
						.desired_width(420.0),
				);

				let can_look_up = !self.loading && !self.url.trim().is_empty();

				if ui
					.add_enabled(can_look_up, egui::Button::new("Look up"))
					.clicked()
				{
					self.loading = true;
					self.lookup = None;
					action = Some(Action::LookUp(self.url.trim().to_owned()));
				}

				if self.loading {
					ui.spinner();
				}
			});

			let Some(lookup) = &self.lookup else {
				return;
			};

			ui.add_space(8.0);
			ui.label(
				RichText::new(lookup.identity.display_name())
					.strong()
					.size(18.0),
			);
			ui.label(RichText::new(format!("Release {}", lookup.tag)).color(theme::muted(ui)));

			if !lookup.discovery_complete {
				ui.label(
					RichText::new("Only the most recent releases were checked.")
						.color(theme::muted(ui)),
				);
			}

			if lookup.options.is_empty() {
				ui.label("This release has no downloadable app files.");

				return;
			}

			ui.add_space(6.0);

			ScrollArea::vertical()
				.auto_shrink([false, false])
				.show(ui, |ui| {
					for option in &lookup.options {
						ui.horizontal_wrapped(|ui| {
							ui.label(RichText::new(&option.asset.name).strong());
							ui.label(
								RichText::new(bytes(option.asset.size)).color(theme::muted(ui)),
							);
							ui.label(
								RichText::new(fit_text(option.fit))
									.small()
									.color(theme::muted(ui)),
							);

							if option.managed {
								if ui.button("Review install").clicked() {
									action = Some(Action::ChooseFile {
										preparation: Box::new(lookup.clone()),
										asset_id: option.asset.id,
									});
								}
							} else {
								ui.label(
									RichText::new("Download only")
										.small()
										.color(theme::muted(ui)),
								);
							}
						});
					}
				});
		});

		action
	}
}

fn fit_text(fit: Fit) -> &'static str {
	match fit {
		Fit::Matches => "Named for this device",
		Fit::Unknown => "Name does not say which device",
		Fit::Mismatch => "Named for another device",
	}
}
