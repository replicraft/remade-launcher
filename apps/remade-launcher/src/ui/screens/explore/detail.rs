//! The detail pane beside the cards.

use egui::{RichText, ScrollArea, Ui};
use launcher_catalog::{Project, State, VerificationLevel};
use launcher_core::{Availability, ExploreRow};

use crate::ui::screens::Action;
use crate::ui::theme;

pub(super) fn detail(
	ui: &mut Ui,
	slug: &str,
	project: &Project,
	availability: Option<&Availability>,
	selected: &mut Option<String>,
) -> Option<Action> {
	let mut action = None;

	ScrollArea::vertical()
		.auto_shrink([false, false])
		.show(ui, |ui| {
			ui.horizontal(|ui| {
				ui.heading(&project.display.name);

				ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
					if ui.button("Close").clicked() {
						*selected = None;
					}
				});
			});

			ui.label(&project.display.summary);
			ui.add_space(8.0);

			fact(ui, "Source", &project.source.display_name());
			fact(ui, "Type", project.display.kind.label());

			if !project.display.categories.is_empty() {
				fact(ui, "Categories", &project.display.categories.join(", "));
			}

			if !project.display.languages.is_empty() {
				fact(ui, "Languages", &project.display.languages.join(", "));
			}

			if let Some(license) = &project.display.license {
				fact(ui, "License", license);
			}

			if let Some(label) = status_label(project.status.state) {
				ui.colored_label(ui.visuals().warn_fg_color, label);

				if let Some(reason) = &project.status.reason {
					ui.label(reason);
				}
			}

			let links = &project.display.links;

			for (label, url) in [
				("Homepage", &links.homepage),
				("Documentation", &links.documentation),
				("Source", &links.source),
				("Changelog", &links.changelog),
			] {
				if let Some(url) = url {
					ui.hyperlink_to(label, url);
				}
			}

			ui.add_space(12.0);
			ui.label(
				RichText::new("Packages")
					.family(crate::ui::theme::semibold())
					.size(16.0),
			);

			if project.variants.is_empty() {
				ui.label(RichText::new("This app has no managed package.").color(theme::muted(ui)));
			}

			for variant in &project.variants {
				ui.label(format!(
					"{} {} · {}",
					variant.os.label(),
					variant.arch.label(),
					variant.format.label()
				));
				ui.label(
					RichText::new(evidence(variant.verification.minimum))
						.small()
						.color(theme::muted(ui)),
				);
			}

			ui.add_space(12.0);

			let installable = project.status.state == State::Active
				&& availability == Some(&Availability::ThisDevice);

			if ui
				.add_enabled(installable, egui::Button::new("Install"))
				.clicked()
			{
				action = Some(Action::Install {
					slug: slug.to_owned(),
				});
			}

			if !installable {
				let reason = match (project.status.state, availability) {
					(State::Active, Some(availability)) => availability_text(availability),
					(State::Active, None) => "Checking this device".to_owned(),
					_ => "Remade has put installs of this app on hold.".to_owned(),
				};

				ui.label(RichText::new(reason).small().color(theme::muted(ui)));
			}
		});

	action
}

/// The detail pane for a demo row, which has no signed project behind it.
pub(super) fn demo_detail(
	ui: &mut Ui,
	row: &ExploreRow,
	selected: &mut Option<String>,
) -> Option<Action> {
	let mut action = None;

	ui.horizontal(|ui| {
		ui.heading(&row.name);

		ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
			if ui.button("Close").clicked() {
				*selected = None;
			}
		});
	});

	ui.label(&row.summary);
	ui.add_space(8.0);

	fact(ui, "Source", &row.repository);
	fact(ui, "Type", row.kind.label());
	fact(ui, "Categories", &row.categories.join(", "));

	if !row.languages.is_empty() {
		fact(ui, "Languages", &row.languages.join(", "));
	}

	if !row.platforms.is_empty() {
		let names: Vec<&str> = row.platforms.iter().map(|os| os.label()).collect();

		fact(ui, "Platforms", &names.join(", "));
	}

	ui.add_space(12.0);

	let installable = row.availability == Availability::ThisDevice;

	if ui
		.add_enabled(installable, egui::Button::new("Install"))
		.clicked()
	{
		action = Some(Action::Install {
			slug: row.slug.clone(),
		});
	}

	if !installable {
		ui.label(
			RichText::new(availability_text(&row.availability))
				.small()
				.color(theme::muted(ui)),
		);
	}

	action
}

fn fact(ui: &mut Ui, label: &str, value: &str) {
	ui.horizontal_wrapped(|ui| {
		ui.label(RichText::new(label).color(theme::muted(ui)));
		ui.label(value);
	});
}

fn evidence(level: VerificationLevel) -> &'static str {
	match level {
		VerificationLevel::HostDigest => "Checked against the digest the download host reports.",
		VerificationLevel::PinnedSha256 => {
			"Checked against a SHA-256 pinned in the signed catalog."
		}
		VerificationLevel::PublisherSignature => "Requires the publisher's own signature.",
	}
}

pub(super) fn status_label(state: State) -> Option<&'static str> {
	match state {
		State::Active => None,
		State::Paused => Some("Paused: new installs are on hold"),
		State::Revoked => Some("Revoked: blocked by Remade"),
	}
}

pub(super) fn availability_text(availability: &Availability) -> String {
	match availability {
		Availability::ThisDevice => "Package available for this device".into(),
		Availability::NeedsNewerLauncher => "Needs a newer version of the launcher".into(),
		Availability::OtherPlatforms(systems) => {
			let names: Vec<_> = systems.iter().map(|os| os.label()).collect();

			format!("Not available here. Packages for {}", names.join(", "))
		}
		Availability::NoPackages => "No managed package".into(),
	}
}
