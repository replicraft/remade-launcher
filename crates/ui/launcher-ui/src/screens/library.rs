use std::collections::HashMap;

use egui::{ProgressBar, RichText, ScrollArea, Ui};
use launcher_core::InstalledApp;
use launcher_model::InstallationId;

use super::{Action, page, unavailable};
use crate::theme;

/// Live progress for an operation, from engine events.
pub(crate) struct Progress {
	pub(crate) label: String,
	pub(crate) fraction: Option<f32>,
}

pub(crate) fn library(
	ui: &mut Ui,
	apps: &[InstalledApp],
	busy: &HashMap<InstallationId, Progress>,
) -> Option<Action> {
	let mut action = None;

	page(ui, "Library", |ui| {
		if apps.is_empty() && busy.is_empty() {
			unavailable(
				ui,
				"Nothing installed yet",
				"Apps you install with Remade Launcher will appear here. Find some in Explore.",
			);

			return;
		}

		ScrollArea::vertical()
			.auto_shrink([false, false])
			.show(ui, |ui| {
				for app in apps {
					action = action.take().or_else(|| row(ui, app, busy.get(&app.id)));
				}
			});
	});

	action
}

fn row(ui: &mut Ui, app: &InstalledApp, progress: Option<&Progress>) -> Option<Action> {
	let mut action = None;

	egui::Frame::new()
		.fill(ui.visuals().widgets.inactive.bg_fill)
		.stroke(ui.visuals().widgets.noninteractive.bg_stroke)
		.corner_radius(10)
		.inner_margin(egui::Margin::symmetric(14, 12))
		.show(ui, |ui| {
			ui.set_width(ui.available_width());

			ui.horizontal(|ui| {
				ui.label(RichText::new(&app.name).strong().size(16.0));
				ui.label(RichText::new(&app.version).color(theme::muted(ui)));
				ui.label(
					RichText::new(&app.repository)
						.small()
						.color(theme::muted(ui)),
				);
			});

			ui.label(
				RichText::new(if app.curated {
					"Reviewed by Remade"
				} else {
					"Added by you. Not reviewed by Remade"
				})
				.small()
				.color(theme::muted(ui)),
			);

			if let Some(progress) = progress {
				ui.add_space(4.0);

				match progress.fraction {
					Some(fraction) => ui.add(ProgressBar::new(fraction).text(&progress.label)),
					None => ui.add(ProgressBar::new(0.0).animate(true).text(&progress.label)),
				};

				return;
			}

			ui.add_space(4.0);

			ui.horizontal(|ui| {
				if ui
					.add_enabled(app.launchable, egui::Button::new("Open"))
					.clicked()
				{
					action = Some(Action::Launch(app.id));
				}

				if app.curated && ui.button("Check for update").clicked() {
					action = Some(Action::Update(app.id));
				}

				if ui.button("Uninstall").clicked() {
					action = Some(Action::Uninstall(app.clone()));
				}
			});
		});

	ui.add_space(6.0);

	action
}
