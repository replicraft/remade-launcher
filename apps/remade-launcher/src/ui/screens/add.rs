//! Add an app from a GitHub repository the user chooses.
//!
//! This path is separate from the catalog. Nothing here is reviewed by
//! Remade, and the screen says so before and after a lookup.

use egui::{RichText, ScrollArea, Ui};
use launcher_core::CustomPreparation;
use launcher_resolver::Fit;

use super::Action;
use super::sheet::bytes;
use crate::ui::theme;

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

		ui.heading(t!("add-title"));
		ui.add_space(8.0);

		(|ui: &mut Ui| {
			ui.label(RichText::new(t!("add-intro")).color(theme::muted(ui)));

			ui.horizontal(|ui| {
				ui.add(
					egui::TextEdit::singleline(&mut self.url)
						.hint_text(t!("add-url-hint"))
						.desired_width(420.0),
				);

				let can_look_up = !self.loading && !self.url.trim().is_empty();

				if ui
					.add_enabled(can_look_up, egui::Button::new(t!("add-look-up")))
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
					.family(crate::ui::theme::semibold())
					.size(18.0),
			);
			ui.label(
				RichText::new(t!("add-release", tag = lookup.tag.as_str())).color(theme::muted(ui)),
			);

			if !lookup.discovery_complete {
				ui.label(RichText::new(t!("add-partial")).color(theme::muted(ui)));
			}

			if lookup.options.is_empty() {
				ui.label(t!("add-no-files"));

				return;
			}

			ui.add_space(6.0);

			ScrollArea::vertical()
				.max_height(260.0)
				.auto_shrink([false, true])
				.show(ui, |ui| {
					for option in &lookup.options {
						ui.horizontal_wrapped(|ui| {
							ui.label(
								RichText::new(&option.asset.name)
									.family(crate::ui::theme::semibold()),
							);
							ui.label(
								RichText::new(bytes(option.asset.size)).color(theme::muted(ui)),
							);
							ui.label(
								RichText::new(fit_text(option.fit))
									.small()
									.color(theme::muted(ui)),
							);

							if option.managed {
								if ui.button(t!("add-review-install")).clicked() {
									action = Some(Action::ChooseFile {
										preparation: Box::new(lookup.clone()),
										asset_id: option.asset.id,
									});
								}
							} else {
								ui.label(
									RichText::new(t!("add-download-only"))
										.small()
										.color(theme::muted(ui)),
								);
							}
						});
					}
				});
		})(ui);

		action
	}
}

fn fit_text(fit: Fit) -> String {
	match fit {
		Fit::Matches => t!("add-fit-matches"),
		Fit::Unknown => t!("add-fit-unknown"),
		Fit::Mismatch => t!("add-fit-mismatch"),
	}
}
