//! Updates: what is available, what needs attention, and what happened.

use std::collections::HashMap;

use egui::{RichText, ScrollArea, Ui};
use launcher_core::{OperationSummary, UpdateState, UpdateStatus};
use launcher_model::{InstallationId, UpdateMode};

use super::library::{Progress, status_text};
use super::sheet::{bytes, when};
use super::{Action, page};
use crate::theme;

pub(crate) struct UpdatesView<'a> {
	pub(crate) statuses: &'a HashMap<InstallationId, UpdateStatus>,
	pub(crate) busy: &'a HashMap<InstallationId, Progress>,
	pub(crate) history: &'a [OperationSummary],
	pub(crate) mode: UpdateMode,
	/// A check has been made at least once this session.
	pub(crate) checked: bool,
}

/// Statuses with something to offer, in a stable order.
pub(crate) fn available(statuses: &HashMap<InstallationId, UpdateStatus>) -> Vec<&UpdateStatus> {
	let mut items: Vec<&UpdateStatus> = statuses
		.values()
		.filter(|status| status.available.is_some() && !matches!(status.state, UpdateState::Pinned))
		.collect();

	items.sort_by_key(|status| status.name.to_lowercase());

	items
}

pub(crate) fn updates(ui: &mut Ui, view: &UpdatesView<'_>) -> Option<Action> {
	let mut action = None;
	let ready = available(view.statuses);

	page(ui, "Updates", |ui| {
		ui.horizontal(|ui| {
			if ui.button("Check for updates").clicked() {
				action = Some(Action::CheckUpdates);
			}

			let label = if ready.is_empty() {
				"Update all".to_owned()
			} else {
				format!("Update all ({})", ready.len())
			};

			if ui
				.add_enabled(!ready.is_empty(), egui::Button::new(label))
				.clicked()
			{
				action = Some(Action::UpdateAll);
			}

			ui.label(
				RichText::new(view.mode.label())
					.small()
					.color(theme::muted(ui)),
			);
		});

		ScrollArea::vertical()
			.auto_shrink([false, false])
			.show(ui, |ui| {
				if ready.is_empty() {
					ui.add_space(16.0);
					ui.label(
						RichText::new(if view.checked {
							"Everything that can be checked is up to date."
						} else {
							"Press Check for updates to look."
						})
						.color(theme::muted(ui)),
					);
				}

				for status in &ready {
					action = action
						.take()
						.or_else(|| available_row(ui, status, view.busy.get(&status.installation)));
				}

				attention(ui, view);
				history(ui, view.history);
			});
	});

	action
}

fn available_row(
	ui: &mut Ui,
	status: &UpdateStatus,
	progress: Option<&Progress>,
) -> Option<Action> {
	let mut action = None;
	let Some(update) = &status.available else {
		return None;
	};

	egui::Frame::new()
		.fill(theme::palette(ui).surface)
		.stroke(ui.visuals().widgets.noninteractive.bg_stroke)
		.corner_radius(10)
		.inner_margin(egui::Margin::symmetric(14, 12))
		.show(ui, |ui| {
			ui.set_width(ui.available_width());

			ui.horizontal(|ui| {
				ui.label(
					RichText::new(&status.name)
						.size(16.0)
						.family(theme::semibold()),
				);
				ui.label(
					RichText::new(format!("{} → {}", status.installed_version, update.version))
						.color(theme::muted(ui)),
				);
				ui.label(
					RichText::new(bytes(update.size))
						.small()
						.color(theme::muted(ui)),
				);
			});

			if let Some(at) = update.published_at {
				ui.label(
					RichText::new(format!("Published {}", when(at)))
						.small()
						.color(theme::muted(ui)),
				);
			}

			if update.rebuilt {
				ui.colored_label(
					ui.visuals().warn_fg_color,
					"The publisher replaced the file for the version you have. Read the notes before updating.",
				);
			}

			if let Some(progress) = progress {
				match progress.fraction {
					Some(fraction) => {
						ui.add(egui::ProgressBar::new(fraction).text(&progress.label))
					}
					None => ui.add(
						egui::ProgressBar::new(0.0)
							.animate(true)
							.text(&progress.label),
					),
				};

				return;
			}

			ui.horizontal(|ui| {
				if ui.button("Update").clicked() {
					action = Some(Action::Update(status.installation));
				}

				if let Some(slug) = &status.catalog_slug
					&& ui.button("Release notes").clicked()
				{
					action = Some(Action::ReleaseNotes {
						slug: slug.clone(),
						tag: update.version.clone(),
						app: status.name.clone(),
					});
				}
			});
		});

	ui.add_space(6.0);

	action
}

/// Apps that could not be checked or are held, never mixed in with the
/// available ones.
fn attention(ui: &mut Ui, view: &UpdatesView<'_>) {
	let mut others: Vec<&UpdateStatus> = view
		.statuses
		.values()
		.filter(|status| {
			matches!(
				status.state,
				UpdateState::CheckFailed(_) | UpdateState::NeedsChoice | UpdateState::Pinned
			) || (status.available.is_some() && matches!(status.state, UpdateState::Pinned))
		})
		.collect();

	if others.is_empty() {
		return;
	}

	others.sort_by_key(|status| status.name.to_lowercase());

	ui.add_space(12.0);
	ui.label(
		RichText::new("Held or not checked")
			.size(16.0)
			.family(theme::semibold()),
	);

	for status in others {
		let (text, attention) = status_text(status, view.mode);
		let color = if attention {
			ui.visuals().warn_fg_color
		} else {
			theme::muted(ui)
		};

		ui.horizontal_wrapped(|ui| {
			ui.label(RichText::new(&status.name).family(theme::semibold()));
			ui.label(RichText::new(text).color(color));
		});
	}
}

fn history(ui: &mut Ui, history: &[OperationSummary]) {
	ui.add_space(12.0);
	ui.label(
		RichText::new("History")
			.size(16.0)
			.family(theme::semibold()),
	);

	if history.is_empty() {
		ui.label(
			RichText::new("Nothing has been installed or removed yet.").color(theme::muted(ui)),
		);

		return;
	}

	for operation in history.iter().take(10) {
		ui.horizontal_wrapped(|ui| {
			ui.label(
				RichText::new(when(operation.started_at))
					.small()
					.color(theme::muted(ui)),
			);
			ui.label(format!("{} {}", operation.kind, operation.state));

			if let Some(app) = &operation.app {
				ui.label(RichText::new(app).color(theme::muted(ui)));
			}

			if let Some(detail) = &operation.detail {
				ui.label(RichText::new(detail).small().color(theme::muted(ui)));
			}
		});
	}
}

#[cfg(test)]
mod tests {
	use launcher_core::{AutoStatus, AvailableUpdate, Decision, SkipReason};
	use launcher_model::MinimumAge;

	use super::*;

	fn status(name: &str, state: UpdateState, with_update: bool) -> UpdateStatus {
		UpdateStatus {
			installation: InstallationId::new(),
			name: name.into(),
			catalog_slug: None,
			installed_version: "v1".into(),
			state,
			available: with_update.then(|| AvailableUpdate {
				version: "v2".into(),
				file: "f".into(),
				size: 1,
				published_at: None,
				rebuilt: false,
			}),
			auto: AutoStatus {
				mode: UpdateMode::Manual,
				minimum_age: MinimumAge::DEFAULT,
				age_is_per_app: false,
				eligible_at: None,
				decision: Decision::Skip(SkipReason::ModeManual),
			},
		}
	}

	#[test]
	fn only_unpinned_apps_with_an_update_are_offered_and_they_sort_by_name() {
		let mut map = HashMap::new();

		for item in [
			status("zed", UpdateState::Available, true),
			status("Alpha", UpdateState::Available, true),
			status("held", UpdateState::Pinned, true),
			status("fine", UpdateState::UpToDate, false),
			status("broken", UpdateState::CheckFailed("x".into()), false),
		] {
			map.insert(item.installation, item);
		}

		let names: Vec<&str> = available(&map)
			.iter()
			.map(|status| status.name.as_str())
			.collect();

		assert_eq!(names, vec!["Alpha", "zed"]);
	}
}
