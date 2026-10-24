//! Explore: search the verified catalog.
//!
//! Search runs on the engine's blocking pool. This screen only sends the
//! query and draws whatever rows last came back, so typing never stalls a
//! frame. There is no install button yet, and the detail pane says so.

use egui::{Frame, Margin, RichText, ScrollArea, Ui};
use launcher_catalog::{Project, State, VerificationLevel};
use launcher_core::{Availability, CatalogState, ExploreQuery, ExploreRow, Origin, ProblemKind};
use time::OffsetDateTime;

use super::{Action, page};
use crate::ui::commands::Commands;
use crate::ui::components::search_bar;
use crate::ui::theme;

const ROW_HEIGHT: f32 = 76.0;
const DETAIL_WIDTH: f32 = 360.0;

pub(crate) struct ExploreView {
	query: String,
	compatible_only: bool,
	rows: Vec<ExploreRow>,
	selected: Option<String>,
	next_request: u64,
	latest_request: u64,
	asked: Option<SearchKey>,
}

/// Everything a result depends on. When it changes, ask again.
#[derive(PartialEq)]
struct SearchKey {
	text: String,
	compatible_only: bool,
	catalog: Option<[u8; 32]>,
}

impl ExploreView {
	pub(crate) fn new() -> Self {
		Self {
			query: String::new(),
			// Explore defaults to the current device. Browsing another
			// platform never changes where something would install.
			compatible_only: true,
			rows: Vec::new(),
			selected: None,
			next_request: 0,
			latest_request: 0,
			asked: None,
		}
	}

	/// Accepts results, ignoring any that a newer query has superseded.
	pub(crate) fn receive(&mut self, request: u64, rows: Vec<ExploreRow>) {
		if request == self.latest_request {
			self.rows = rows;
		}
	}

	/// Shows the catalog.
	pub(crate) fn show(&mut self, ui: &mut Ui, commands: &Commands) -> Option<Action> {
		let mut action = None;
		let state = commands.catalog_state();

		self.search_if_changed(&state, commands);

		page(ui, "Explore", |ui| {
			banner(ui, &state, commands);

			search_bar(ui, "Search the catalog", &mut self.query);
			ui.add_space(4.0);

			ui.checkbox(&mut self.compatible_only, "Only apps for this device");

			if state.catalog.is_none() {
				return;
			}

			ui.add_space(4.0);

			let has_detail = self.selected.is_some();
			let list_width = if has_detail {
				(ui.available_width() - DETAIL_WIDTH - 24.0).max(320.0)
			} else {
				ui.available_width()
			};

			ui.horizontal_top(|ui| {
				ui.vertical(|ui| {
					ui.set_width(list_width);
					self.list(ui);
				});

				if let Some(slug) = self.selected.clone() {
					ui.separator();

					ui.vertical(|ui| {
						ui.set_width(DETAIL_WIDTH);

						let project = state
							.catalog
							.as_ref()
							.and_then(|catalog| catalog.project(&slug));

						let availability = self
							.rows
							.iter()
							.find(|row| row.slug == slug)
							.map(|row| row.availability.clone());

						match project {
							Some(project) => {
								action = detail(
									ui,
									&slug,
									project,
									availability.as_ref(),
									&mut self.selected,
								);
							}
							None => self.selected = None,
						}
					});
				}
			});
		});

		action
	}

	fn search_if_changed(&mut self, state: &CatalogState, commands: &Commands) {
		let key = SearchKey {
			text: self.query.clone(),
			compatible_only: self.compatible_only,
			catalog: state.catalog.as_ref().map(|catalog| catalog.digest()),
		};

		if self.asked.as_ref() == Some(&key) {
			return;
		}

		self.next_request += 1;
		self.latest_request = self.next_request;

		commands.search(
			self.latest_request,
			ExploreQuery {
				text: key.text.clone(),
				compatible_only: key.compatible_only,
			},
		);

		self.asked = Some(key);
	}

	fn list(&mut self, ui: &mut Ui) {
		if self.rows.is_empty() {
			ui.add_space(16.0);
			ui.label(RichText::new("No apps match.").color(theme::muted(ui)));

			if self.compatible_only {
				ui.label(
					RichText::new(
						"Turn off \"Only apps for this device\" to see every app in the catalog.",
					)
					.color(theme::muted(ui)),
				);
			}

			return;
		}

		ScrollArea::vertical()
			.auto_shrink([false, false])
			.show_rows(ui, ROW_HEIGHT, self.rows.len(), |ui, range| {
				for row in &self.rows[range] {
					let chosen = self.selected.as_deref() == Some(row.slug.as_str());

					if card(ui, row, chosen) {
						self.selected = Some(row.slug.clone());
					}
				}
			});
	}
}

/// One catalog entry. Returns true when it was clicked.
fn card(ui: &mut Ui, row: &ExploreRow, selected: bool) -> bool {
	let stroke_color = if selected {
		ui.visuals().selection.bg_fill
	} else {
		ui.visuals().widgets.noninteractive.bg_stroke.color
	};

	let response = Frame::new()
		.fill(ui.visuals().widgets.inactive.bg_fill)
		.stroke(egui::Stroke::new(
			if selected { 1.5 } else { 1.0 },
			stroke_color,
		))
		.corner_radius(10)
		.inner_margin(Margin::symmetric(14, 10))
		.show(ui, |ui| {
			ui.set_width(ui.available_width());
			ui.set_height(ROW_HEIGHT - 20.0);

			ui.horizontal(|ui| {
				ui.label(
					RichText::new(&row.name)
						.family(crate::ui::theme::semibold())
						.size(16.0),
				);
				// The signed repository identity sits beside the name so an
				// unsigned description can never stand in for it.
				ui.label(
					RichText::new(&row.repository)
						.small()
						.color(theme::muted(ui)),
				);

				if let Some(label) = status_label(row.status) {
					ui.colored_label(ui.visuals().warn_fg_color, label);
				}
			});

			ui.label(RichText::new(&row.summary).color(theme::muted(ui)));
			ui.label(RichText::new(availability_text(&row.availability)).small());
		})
		.response
		.interact(egui::Sense::click());

	response
		.on_hover_cursor(egui::CursorIcon::PointingHand)
		.clicked()
}

fn detail(
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

fn status_label(state: State) -> Option<&'static str> {
	match state {
		State::Active => None,
		State::Paused => Some("Paused: new installs are on hold"),
		State::Revoked => Some("Revoked: blocked by Remade"),
	}
}

fn availability_text(availability: &Availability) -> String {
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

/// What the user needs to know about the catalog itself. Network trouble and
/// rejected bytes are worded differently, because they mean different things.
fn banner(ui: &mut Ui, state: &CatalogState, commands: &Commands) {
	let mut lines: Vec<(bool, String)> = Vec::new();

	if let (Some(catalog), Some(origin)) = (&state.catalog, &state.origin) {
		let expired = state.freshness(OffsetDateTime::now_utc())
			== Some(launcher_catalog::Freshness::Expired);

		lines.push((
			false,
			match origin {
				Origin::Network { at } => format!("Verified catalog, checked {}.", date(*at)),
				Origin::Cache { accepted_at } => {
					format!("Using verified catalog cached on {}.", date(*accepted_at))
				}
			},
		));

		if expired {
			lines.push((
				true,
				format!(
					"This catalog expired on {}. You can browse it, but new installs and updates are on hold.",
					date(catalog.expires_at())
				),
			));
		}
	}

	if let Some(problem) = &state.problem {
		let heading = match problem.kind {
			ProblemKind::Network => "Offline",
			ProblemKind::Security => "Catalog rejected",
			ProblemKind::Unusable => "Catalog not usable",
			ProblemKind::NoTrustedKeys => "Catalog not configured",
			ProblemKind::Storage => "Storage problem",
		};

		lines.push((
			problem.kind != ProblemKind::Network,
			format!("{heading}. {}", problem.message),
		));
	}

	if state.clock_warning {
		lines.push((true, "Your computer's clock is earlier than a time the launcher already trusted. Check the date and time.".into()));
	}

	if lines.is_empty() && state.catalog.is_none() && !state.refreshing {
		lines.push((false, "No catalog has been loaded yet.".into()));
	}

	for (is_warning, text) in lines {
		let color = if is_warning {
			ui.visuals().warn_fg_color
		} else {
			theme::muted(ui)
		};

		ui.colored_label(color, text);
	}

	ui.horizontal(|ui| {
		if state.refreshing {
			ui.spinner();
			ui.label("Checking for catalog updates…");
		} else if ui.button("Check for updates").clicked() {
			commands.refresh_catalog();
		}
	});
}

fn date(time: OffsetDateTime) -> String {
	format!(
		"{}-{:02}-{:02}",
		time.year(),
		u8::from(time.month()),
		time.day()
	)
}
