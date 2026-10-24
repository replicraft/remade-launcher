//! The page for one app, reached by choosing it in the catalog.
//!
//! A real entry and a demo entry fill the same [`Facts`], so they share one
//! layout.

use egui::{Align, Frame, Layout, Margin, RichText, ScrollArea, Ui, vec2};
use launcher_catalog::{Project, State, VerificationLevel};
use launcher_core::{Availability, ExploreRow};
use launcher_model::Os;

use super::filters::category_label;
use crate::ui::components::{app_tile_for, platform_badges, primary_button, tag, verified_badge};
use crate::ui::screens::Action;
use crate::ui::{icons, theme};

/// Everything the page shows about one app.
struct Facts<'a> {
	slug: &'a str,
	name: &'a str,
	summary: &'a str,
	kind: &'a str,
	source: String,
	categories: &'a [String],
	languages: &'a [String],
	platforms: Vec<Os>,
	license: Option<&'a str>,
	links: Vec<(&'static str, &'a str)>,
	/// One entry per package: what it is, and how it is checked.
	packages: Vec<(String, &'static str)>,
	/// Why installs are on hold, if they are.
	hold: Option<String>,
	availability: Option<&'a Availability>,
	active: bool,
}

pub(super) fn detail(
	ui: &mut Ui,
	slug: &str,
	project: &Project,
	availability: Option<&Availability>,
) -> Option<Action> {
	let display = &project.display;
	let links = &display.links;

	let mut hold = status_label(project.status.state).map(str::to_owned);

	if let (Some(label), Some(reason)) = (&mut hold, &project.status.reason) {
		label.push_str(&format!(". {reason}"));
	}

	let mut platforms: Vec<Os> = Vec::new();

	for variant in &project.variants {
		if !platforms.contains(&variant.os) {
			platforms.push(variant.os);
		}
	}

	page(
		ui,
		&Facts {
			slug,
			name: &display.name,
			summary: &display.summary,
			kind: display.kind.label(),
			source: project.source.display_name(),
			categories: &display.categories,
			languages: &display.languages,
			platforms,
			license: display.license.as_deref(),
			links: [
				("Homepage", &links.homepage),
				("Documentation", &links.documentation),
				("Source", &links.source),
				("Changelog", &links.changelog),
			]
			.into_iter()
			.filter_map(|(label, url)| url.as_deref().map(|url| (label, url)))
			.collect(),
			packages: project
				.variants
				.iter()
				.map(|variant| {
					(
						format!(
							"{} {} · {}",
							variant.os.label(),
							variant.arch.label(),
							variant.format.label()
						),
						evidence(variant.verification.minimum),
					)
				})
				.collect(),
			hold,
			availability,
			active: project.status.state == State::Active,
		},
	)
}

/// The page for a demo row, which has no signed project behind it.
pub(super) fn demo_detail(ui: &mut Ui, row: &ExploreRow) -> Option<Action> {
	page(
		ui,
		&Facts {
			slug: &row.slug,
			name: &row.name,
			summary: &row.summary,
			kind: row.kind.label(),
			source: row.repository.clone(),
			categories: &row.categories,
			languages: &row.languages,
			platforms: row.platforms.clone(),
			license: Some("MIT"),
			links: vec![
				("Homepage", "https://example.com"),
				("Source", "https://example.com"),
			],
			packages: row
				.platforms
				.iter()
				.map(|os| {
					(
						format!("{} · Universal", os.label()),
						"Checked against a SHA-256 pinned in the signed catalog.",
					)
				})
				.collect(),
			hold: None,
			availability: Some(&row.availability),
			active: true,
		},
	)
}

fn page(ui: &mut Ui, facts: &Facts<'_>) -> Option<Action> {
	let mut action = None;
	let installable = facts.active && facts.availability == Some(&Availability::ThisDevice);

	ScrollArea::vertical()
		.auto_shrink([false, false])
		.show(ui, |ui| {
			header(ui, facts, installable, &mut action);

			if let Some(hold) = &facts.hold {
				ui.add_space(10.0);
				ui.colored_label(ui.visuals().warn_fg_color, hold);
			}

			ui.add_space(16.0);
			card(ui, "Details", |ui| details(ui, facts));

			if !facts.links.is_empty() {
				ui.add_space(14.0);

				ui.horizontal_wrapped(|ui| {
					ui.spacing_mut().item_spacing.x = 18.0;

					for (label, url) in &facts.links {
						ui.hyperlink_to(format!("{}  {label}", icons::ARROW_SQUARE_OUT), *url);
					}
				});
			}

			ui.add_space(14.0);
			card(ui, "Packages", |ui| packages(ui, facts));
			ui.add_space(24.0);
		});

	action
}

/// The icon, name, and summary on the left; Install on the right.
fn header(ui: &mut Ui, facts: &Facts<'_>, installable: bool, action: &mut Option<Action>) {
	ui.horizontal_top(|ui| {
		app_tile_for(ui, Some(facts.slug), facts.name, 84.0);
		ui.add_space(8.0);

		ui.vertical(|ui| {
			ui.set_max_width((ui.available_width() - 240.0).max(240.0));
			ui.spacing_mut().item_spacing.y = 4.0;

			ui.horizontal(|ui| {
				ui.label(
					RichText::new(facts.name)
						.size(28.0)
						.family(theme::semibold()),
				);
				verified_badge(ui, true);
			});

			ui.label(
				RichText::new(facts.summary)
					.size(15.0)
					.color(theme::muted(ui)),
			);
			ui.add_space(4.0);

			ui.horizontal_wrapped(|ui| {
				tag(ui, facts.kind);

				for category in facts.categories {
					tag(ui, &category_label(category));
				}

				if let Some(license) = facts.license {
					tag(ui, license);
				}
			});
		});

		ui.with_layout(Layout::right_to_left(Align::Min), |ui| {
			ui.vertical(|ui| {
				ui.spacing_mut().item_spacing.y = 6.0;

				let label = format!("{}  Install", icons::DOWNLOAD);

				if primary_button(ui, &label, installable, vec2(210.0, 54.0)).clicked() {
					*action = Some(Action::Install {
						slug: facts.slug.to_owned(),
					});
				}

				if !installable {
					let reason = match (facts.active, facts.availability) {
						(true, Some(availability)) => availability_text(availability),
						(true, None) => "Checking this device".to_owned(),
						_ => "Remade has put installs of this app on hold.".to_owned(),
					};

					ui.add(
						egui::Label::new(RichText::new(reason).small().color(theme::muted(ui)))
							.wrap(),
					);
				}
			});
		});
	});
}

fn details(ui: &mut Ui, facts: &Facts<'_>) {
	egui::Grid::new("app-details")
		.num_columns(2)
		.spacing([32.0, 12.0])
		.min_col_width(110.0)
		.show(ui, |ui| {
			row(ui, "Source", |ui| {
				ui.label(&facts.source);
			});

			row(ui, "Type", |ui| {
				ui.label(facts.kind);
			});

			if !facts.categories.is_empty() {
				row(ui, "Categories", |ui| {
					ui.label(
						facts
							.categories
							.iter()
							.map(|slug| category_label(slug))
							.collect::<Vec<_>>()
							.join(", "),
					);
				});
			}

			if !facts.languages.is_empty() {
				row(ui, "Languages", |ui| {
					ui.label(facts.languages.join(", "));
				});
			}

			if !facts.platforms.is_empty() {
				row(ui, "Platforms", |ui| {
					platform_badges(ui, &facts.platforms);
					ui.label(
						facts
							.platforms
							.iter()
							.map(|os| os.label())
							.collect::<Vec<_>>()
							.join(", "),
					);
				});
			}

			if let Some(license) = facts.license {
				row(ui, "License", |ui| {
					ui.label(license);
				});
			}
		});
}

fn packages(ui: &mut Ui, facts: &Facts<'_>) {
	if facts.packages.is_empty() {
		ui.label(RichText::new("This app has no managed package.").color(theme::muted(ui)));

		return;
	}

	for (index, (title, how)) in facts.packages.iter().enumerate() {
		if index > 0 {
			ui.add_space(8.0);
		}

		ui.label(RichText::new(title).family(theme::semibold()));
		ui.label(RichText::new(*how).small().color(theme::muted(ui)));
	}
}

/// A titled panel.
fn card(ui: &mut Ui, title: &str, contents: impl FnOnce(&mut Ui)) {
	let palette = theme::palette(ui);

	Frame::new()
		.fill(palette.surface)
		.stroke(egui::Stroke::new(1.0, palette.border))
		.corner_radius(12)
		.inner_margin(Margin::same(20))
		.show(ui, |ui| {
			ui.set_width(ui.available_width());
			ui.label(RichText::new(title).size(17.0).family(theme::semibold()));
			ui.add_space(12.0);

			contents(ui);
		});
}

/// One row of the details grid: a muted label, then its value.
fn row(ui: &mut Ui, label: &str, value: impl FnOnce(&mut Ui)) {
	ui.label(RichText::new(label).color(theme::muted(ui)));
	ui.horizontal(|ui| value(ui));
	ui.end_row();
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

/// Why an app can or cannot be installed on this device, in full.
pub(super) fn availability_text(availability: &Availability) -> String {
	match availability {
		Availability::ThisDevice => "A package exists for this device.".into(),
		Availability::NeedsNewerLauncher => "Needs a newer version of the launcher.".into(),
		Availability::OtherPlatforms(systems) => {
			let names: Vec<_> = systems.iter().map(|os| os.label()).collect();

			format!(
				"No package for this device. Packages exist for {}.",
				names.join(", ")
			)
		}
		Availability::NoPackages => "No managed package yet.".into(),
	}
}
