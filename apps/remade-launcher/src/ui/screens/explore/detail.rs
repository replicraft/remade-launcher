//! The page for one app, reached by choosing it in the catalog.
//!
//! A real entry and a demo entry fill the same [`Facts`], so they share one
//! layout.

use egui::{Align, Layout, RichText, ScrollArea, Ui, vec2};
use launcher_catalog::{Maturity, Project, State, VerificationLevel};
use launcher_core::{Availability, ExploreRow};
use launcher_model::Os;

use crate::ui::i18n::text_for;

use super::filters::{category_label, kind_name, maturity_name};
use crate::ui::components::{
	app_tile_for, panel, platform_badges, primary_button, quiet_button, quiet_button_with_icon,
	tag, verified_badge,
};
use crate::ui::screens::Action;
use crate::ui::{icons, theme};

/// Everything the page shows about one app.
struct Facts<'a> {
	slug: &'a str,
	name: &'a str,
	summary: &'a str,
	description: Option<String>,
	kind: String,
	source: String,
	categories: &'a [String],
	maturity: Option<Maturity>,
	languages: &'a [String],
	/// The original software it remakes or replaces.
	originals: &'a [String],
	platforms: Vec<Os>,
	license: Option<&'a str>,
	links: Vec<(String, &'a str)>,
	/// One entry per package: what it is, and how it is checked.
	packages: Vec<(String, String)>,
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

	let hold = status_label(project.status.state).map(|label| match &project.status.reason {
		Some(reason) => t!(
			"explore-hold-with-reason",
			label = label,
			reason = reason.as_str()
		),
		None => label,
	});

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
			description: display.description.clone(),
			kind: kind_name(display.kind),
			source: project.source.display_name(),
			categories: &display.categories,
			maturity: display.maturity,
			languages: &display.languages,
			originals: &display.original_names,
			platforms,
			license: display.license.as_deref(),
			links: [
				("homepage", &links.homepage),
				("documentation", &links.documentation),
				("source", &links.source),
				("changelog", &links.changelog),
			]
			.into_iter()
			.filter_map(|(name, url)| {
				url.as_deref()
					.map(|url| (text_for("explore-link", name), url))
			})
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
			description: Some(crate::ui::demo::description(&row.name)),
			kind: kind_name(row.kind),
			source: row.repository.clone(),
			categories: &row.categories,
			maturity: row.maturity,
			languages: &row.languages,
			originals: &row.originals,
			platforms: row.platforms.clone(),
			license: Some("MIT"),
			links: vec![
				(text_for("explore-link", "homepage"), "https://example.com"),
				(text_for("explore-link", "source"), "https://example.com"),
			],
			packages: row
				.platforms
				.iter()
				.map(|os| {
					(
						format!("{} · Universal", os.label()),
						evidence(VerificationLevel::PinnedSha256),
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

			if !facts.links.is_empty() {
				ui.add_space(16.0);

				ui.horizontal_wrapped(|ui| {
					ui.spacing_mut().item_spacing.x = 20.0;

					for (label, url) in &facts.links {
						ui.hyperlink_to(format!("{}  {label}", icons::ARROW_SQUARE_OUT), *url);
					}
				});
			}

			if let Some(description) = &facts.description {
				ui.add_space(16.0);
				card(ui, &t!("explore-description"), |ui| {
					description_text(ui, description)
				});
			}

			ui.add_space(16.0);
			card(ui, &t!("explore-details"), |ui| details(ui, facts));

			ui.add_space(14.0);
			card(ui, &t!("explore-packages"), |ui| packages(ui, facts));
			// A quiet second Install, for when the person has read this far.
			ui.add_space(16.0);

			let label = format!("{}  {}", icons::DOWNLOAD, t!("explore-install"));

			ui.horizontal(|ui| {
				if quiet_button(ui, &label, installable, vec2(140.0, 38.0)).clicked() {
					action = Some(Action::Install {
						slug: facts.slug.to_owned(),
					});
				}

				// Only the flag is red, so the label stays calm.
				let flag = (icons::FLAG, theme::palette(ui).danger);

				if quiet_button_with_icon(ui, flag, &t!("explore-report"), true, vec2(160.0, 38.0))
					.on_hover_text(t!("explore-report.tooltip"))
					.clicked()
				{
					ui.ctx().open_url(egui::OpenUrl::new_tab(report_url(facts)));
				}
			});

			ui.add_space(24.0);
		});

	action
}

/// The icon, name, and summary on the left; Install on the right.
fn header(ui: &mut Ui, facts: &Facts<'_>, installable: bool, action: &mut Option<Action>) {
	// The left side is given a definite width that stops well short of the
	// button, so the text can never run up against Install.
	const BUTTON_WIDTH: f32 = 240.0;
	const GAP: f32 = 56.0;
	const TILE: f32 = 84.0;

	let left = (ui.available_width() - BUTTON_WIDTH - GAP).max(280.0);

	ui.horizontal_top(|ui| {
		ui.allocate_ui_with_layout(vec2(left, 0.0), Layout::left_to_right(Align::Min), |ui| {
			ui.set_width(left);

			app_tile_for(ui, Some(facts.slug), facts.name, TILE);
			ui.add_space(8.0);

			header_text(ui, facts, left - TILE - 8.0);
		});

		ui.with_layout(Layout::right_to_left(Align::Min), |ui| {
			ui.vertical(|ui| {
				ui.spacing_mut().item_spacing.y = 6.0;

				let label = format!("{}  {}", icons::DOWNLOAD, t!("explore-install"));

				if primary_button(ui, &label, installable, vec2(BUTTON_WIDTH, 60.0)).clicked() {
					*action = Some(Action::Install {
						slug: facts.slug.to_owned(),
					});
				}

				if !installable {
					let reason = match (facts.active, facts.availability) {
						(true, Some(availability)) => availability_text(availability),
						(true, None) => t!("explore-checking-device"),
						_ => t!("explore-on-hold"),
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

/// The name, summary, and tags, wrapped to `width`.
fn header_text(ui: &mut Ui, facts: &Facts<'_>, width: f32) {
	ui.vertical(|ui| {
		ui.set_max_width(width);
		ui.spacing_mut().item_spacing.y = 4.0;

		ui.horizontal(|ui| {
			ui.label(
				RichText::new(facts.name)
					.size(28.0)
					.family(theme::semibold()),
			);
			verified_badge(ui, true);
		});

		ui.add(
			egui::Label::new(
				RichText::new(facts.summary)
					.size(15.0)
					.color(theme::muted(ui)),
			)
			.wrap(),
		);
		ui.add_space(4.0);

		ui.horizontal_wrapped(|ui| {
			tag(ui, &facts.kind);

			if let Some(maturity) = facts.maturity {
				tag(ui, &maturity_name(maturity));
			}

			for category in facts.categories {
				tag(ui, &category_label(category));
			}
		});
	});
}

/// Paragraphs are separated by blank lines.
fn description_text(ui: &mut Ui, text: &str) {
	for (index, paragraph) in text
		.split("\n\n")
		.map(str::trim)
		.filter(|p| !p.is_empty())
		.enumerate()
	{
		if index > 0 {
			ui.add_space(10.0);
		}

		ui.add(egui::Label::new(RichText::new(paragraph).size(15.0)).wrap());
	}
}

fn details(ui: &mut Ui, facts: &Facts<'_>) {
	egui::Grid::new("app-details")
		.num_columns(2)
		.spacing([32.0, 12.0])
		.min_col_width(110.0)
		.show(ui, |ui| {
			row(ui, &t!("explore-detail-source"), |ui| {
				ui.label(&facts.source);
			});

			row(ui, &t!("explore-detail-type"), |ui| {
				ui.label(&facts.kind);
			});

			if !facts.categories.is_empty() {
				row(ui, &t!("explore-detail-categories"), |ui| {
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

			if let Some(maturity) = facts.maturity {
				row(ui, &t!("explore-detail-maturity"), |ui| {
					ui.label(maturity_name(maturity));
				});
			}

			if !facts.originals.is_empty() {
				row(ui, &t!("explore-detail-originals"), |ui| {
					ui.label(facts.originals.join(", "));
				});
			}

			if !facts.languages.is_empty() {
				row(ui, &t!("explore-detail-languages"), |ui| {
					ui.label(facts.languages.join(", "));
				});
			}

			if !facts.platforms.is_empty() {
				row(ui, &t!("explore-detail-platforms"), |ui| {
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
				row(ui, &t!("explore-detail-license"), |ui| {
					ui.label(license);
				});
			}
		});
}

fn packages(ui: &mut Ui, facts: &Facts<'_>) {
	if facts.packages.is_empty() {
		ui.label(RichText::new(t!("explore-no-package")).color(theme::muted(ui)));

		return;
	}

	for (index, (title, how)) in facts.packages.iter().enumerate() {
		if index > 0 {
			ui.add_space(8.0);
		}

		ui.label(RichText::new(title).family(theme::semibold()));
		ui.label(RichText::new(how).small().color(theme::muted(ui)));
	}
}

fn card(ui: &mut Ui, title: &str, contents: impl FnOnce(&mut Ui)) {
	panel(ui, title, None, contents);
}

/// One row of the details grid: a muted label, then its value.
fn row(ui: &mut Ui, label: &str, value: impl FnOnce(&mut Ui)) {
	ui.label(RichText::new(label).color(theme::muted(ui)));
	ui.horizontal(|ui| value(ui));
	ui.end_row();
}

fn evidence(level: VerificationLevel) -> String {
	match level {
		VerificationLevel::HostDigest => t!("explore-evidence-host"),
		VerificationLevel::PinnedSha256 => t!("explore-evidence-pinned"),
		VerificationLevel::PublisherSignature => t!("explore-evidence-publisher"),
	}
}

pub(super) fn status_label(state: State) -> Option<String> {
	match state {
		State::Active => None,
		State::Paused => Some(t!("explore-status-paused")),
		State::Revoked => Some(t!("explore-status-revoked")),
	}
}

/// Why an app can or cannot be installed on this device, in full.
pub(super) fn availability_text(availability: &Availability) -> String {
	match availability {
		Availability::ThisDevice => t!("explore-availability-this-device"),
		Availability::NeedsNewerLauncher => t!("explore-availability-newer-launcher"),
		Availability::OtherPlatforms(systems) => {
			let names: Vec<_> = systems.iter().map(|os| os.label()).collect();

			t!(
				"explore-availability-other-platforms",
				systems = names.join(", ")
			)
		}
		Availability::NoPackages => t!("explore-availability-no-package"),
	}
}

/// The launcher's issue form, with the entry named in the title so a report
/// says which listing it is about. Only public catalog facts are included.
fn report_url(facts: &Facts<'_>) -> String {
	let base = format!("{}/issues/new", env!("CARGO_PKG_REPOSITORY"));

	match launcher_net::Url::parse(&base) {
		Ok(mut url) => {
			url.query_pairs_mut().append_pair(
				"title",
				&format!("Catalog entry: {} ({})", facts.name, facts.slug),
			);

			url.into()
		}
		Err(_) => base,
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn the_report_link_goes_to_the_launcher_issue_form_and_names_the_entry() {
		let facts = Facts {
			slug: "lumen-notes",
			name: "Lumen & Notes",
			summary: "",
			description: None,
			kind: "Desktop app".to_owned(),
			source: String::new(),
			categories: &[],
			maturity: None,
			languages: &[],
			originals: &[],
			platforms: Vec::new(),
			license: None,
			links: Vec::new(),
			packages: Vec::new(),
			hold: None,
			availability: None,
			active: true,
		};
		let link = report_url(&facts);

		assert!(
			link.starts_with("https://github.com/replicraft/remade-launcher/issues/new?title="),
			"{link}"
		);
		assert!(link.contains("lumen-notes"), "{link}");
		assert!(
			!link.contains("Lumen & Notes"),
			"the title must be escaped: {link}"
		);
	}
}
