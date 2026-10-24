use std::collections::HashMap;

use egui::{ComboBox, ProgressBar, RichText, ScrollArea, Ui};
use launcher_core::{Decision, InstalledApp, SkipReason, UpdateState, UpdateStatus};
use launcher_model::{InstallationId, MinimumAge, UpdateMode};
use launcher_storage::AppPolicy;

use super::{Action, page, unavailable};
use crate::theme;

/// Live progress for an operation, from engine events.
pub(crate) struct Progress {
	pub(crate) label: String,
	pub(crate) fraction: Option<f32>,
	/// Stopping is still safe: nothing has moved into place yet.
	pub(crate) stoppable: bool,
}

pub(crate) struct LibraryView<'a> {
	pub(crate) apps: &'a [InstalledApp],
	pub(crate) busy: &'a HashMap<InstallationId, Progress>,
	pub(crate) statuses: &'a HashMap<InstallationId, UpdateStatus>,
	pub(crate) policies: &'a HashMap<InstallationId, AppPolicy>,
	pub(crate) mode: UpdateMode,
	pub(crate) global_age: MinimumAge,
}

pub(crate) fn library(ui: &mut Ui, view: &LibraryView<'_>) -> Option<Action> {
	let mut action = None;

	page(ui, "Library", |ui| {
		if view.apps.is_empty() && view.busy.is_empty() {
			unavailable(
				ui,
				"Nothing installed yet",
				"Apps you install with Remade Launcher will appear here. Find some in Explore.",
			);

			return;
		}

		ui.label(
			RichText::new(view.mode.label())
				.small()
				.color(theme::muted(ui)),
		);

		ScrollArea::vertical()
			.auto_shrink([false, false])
			.show(ui, |ui| {
				for app in view.apps {
					action = action.take().or_else(|| row(ui, app, view));
				}
			});
	});

	action
}

/// What to tell a person about an app's update, in the words the settings use.
/// A failed check is never "up to date".
pub(crate) fn status_text(status: &UpdateStatus, mode: UpdateMode) -> (String, bool) {
	match &status.state {
		UpdateState::UpToDate => ("Up to date".into(), false),
		UpdateState::Pinned => ("Pinned".into(), false),
		UpdateState::CheckFailed(why) => (
			format!("Needs your attention: could not check ({why})"),
			true,
		),
		UpdateState::Unsupported(why) => (why.clone(), false),
		UpdateState::NeedsChoice => ("Needs your attention: several packages fit".into(), true),
		UpdateState::Available => {
			let version = status
				.available
				.as_ref()
				.map_or("?", |available| available.version.as_str());

			if !mode.is_automatic() {
				return (format!("Update available: {version}"), false);
			}

			match &status.auto.decision {
				Decision::Apply => (
					format!("Update available: {version}. It will install automatically."),
					false,
				),
				Decision::Skip(SkipReason::MinReleaseAgeNotMet { .. }) => {
					(format!("Waiting for minimum age: {version}"), false)
				}
				Decision::Skip(SkipReason::AutoPaused) => (
					format!("Update available: {version}. Automatic updates are paused."),
					false,
				),
				Decision::Skip(reason) => {
					(format!("Needs your attention: {}", reason.message()), true)
				}
			}
		}
	}
}

fn row(ui: &mut Ui, app: &InstalledApp, view: &LibraryView<'_>) -> Option<Action> {
	let mut action = None;
	let progress = view.busy.get(&app.id);
	let status = view.statuses.get(&app.id);

	egui::Frame::new()
		.fill(ui.visuals().widgets.inactive.bg_fill)
		.stroke(ui.visuals().widgets.noninteractive.bg_stroke)
		.corner_radius(10)
		.inner_margin(egui::Margin::symmetric(14, 12))
		.show(ui, |ui| {
			ui.set_width(ui.available_width());

			ui.horizontal(|ui| {
				ui.label(
					RichText::new(&app.name)
						.family(crate::theme::semibold())
						.size(16.0),
				);
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

				if progress.stoppable && ui.small_button("Stop").clicked() {
					action = Some(Action::Cancel);
				}

				return;
			}

			if let Some(status) = status {
				let (text, attention) = status_text(status, view.mode);
				let color = if attention {
					ui.visuals().warn_fg_color
				} else {
					theme::muted(ui)
				};

				ui.label(RichText::new(text).small().color(color));
			}

			ui.add_space(4.0);

			ui.horizontal(|ui| {
				if ui
					.add_enabled(app.launchable, egui::Button::new("Open"))
					.clicked()
				{
					action = Some(Action::Launch(app.id));
				}

				let update_ready = status.is_some_and(|status| status.available.is_some());
				let pinned = view
					.policies
					.get(&app.id)
					.is_some_and(|policy| policy.pinned_release_id.is_some());

				if app.curated {
					let label = if update_ready {
						"Update"
					} else {
						"Check for update"
					};

					if ui.add_enabled(!pinned, egui::Button::new(label)).clicked() {
						action = Some(Action::Update(app.id));
					}
				}

				if ui.button("Uninstall").clicked() {
					action = Some(Action::Uninstall(app.clone()));
				}
			});

			if app.curated {
				update_settings(ui, app, view, &mut action);
			}
		});

	ui.add_space(6.0);

	action
}

/// One app's own update preferences. They can pause or delay an update,
/// never authorize one.
fn update_settings(
	ui: &mut Ui,
	app: &InstalledApp,
	view: &LibraryView<'_>,
	action: &mut Option<Action>,
) {
	let policy = view.policies.get(&app.id).cloned().unwrap_or_default();

	ui.collapsing("Update settings", |ui| {
		let mut changed = policy.clone();

		let mut pinned = policy.pinned_release_id.is_some();

		if ui.checkbox(&mut pinned, "Pin at this version").changed() {
			*action = Some(Action::Pin(app.id, pinned));
		}

		ui.checkbox(
			&mut changed.auto_paused,
			"Pause automatic updates for this app",
		);
		ui.checkbox(&mut changed.allow_prerelease, "Allow pre-releases");

		ui.horizontal(|ui| {
			ui.label("Minimum release age");

			let effective = match changed.min_age_seconds.and_then(MinimumAge::from_seconds) {
				Some(age) => preset_label(age),
				None => format!("Use global default ({})", preset_label(view.global_age)),
			};

			ComboBox::from_id_salt(("age", app.id.to_string()))
				.selected_text(effective)
				.show_ui(ui, |ui| {
					if ui
						.selectable_label(changed.min_age_seconds.is_none(), "Use global default")
						.clicked()
					{
						changed.min_age_seconds = None;
					}

					for (label, age) in MinimumAge::PRESETS {
						if ui
							.selectable_label(changed.min_age_seconds == Some(age.seconds()), label)
							.clicked()
						{
							changed.min_age_seconds = Some(age.seconds());
						}
					}
				});
		});

		if changed != policy {
			*action = Some(Action::SetPolicy(app.id, changed));
		}
	});
}

fn preset_label(age: MinimumAge) -> String {
	MinimumAge::PRESETS
		.iter()
		.find(|(_, preset)| *preset == age)
		.map_or_else(|| age.short(), |(label, _)| (*label).to_owned())
}

#[cfg(test)]
mod tests {
	use launcher_core::{AutoStatus, AvailableUpdate};

	use super::*;

	fn status(state: UpdateState, decision: Decision, available: bool) -> UpdateStatus {
		UpdateStatus {
			installation: InstallationId::new(),
			name: "App".into(),
			catalog_slug: Some("app".into()),
			installed_version: "v1".into(),
			state,
			available: available.then(|| AvailableUpdate {
				version: "v2".into(),
				file: "a.zip".into(),
				size: 1,
				published_at: None,
				rebuilt: false,
			}),
			auto: AutoStatus {
				mode: UpdateMode::OnLaunch,
				minimum_age: MinimumAge::DEFAULT,
				age_is_per_app: false,
				eligible_at: None,
				decision,
			},
		}
	}

	#[test]
	fn a_failed_check_is_never_called_up_to_date() {
		let failed = status(
			UpdateState::CheckFailed("offline".into()),
			Decision::Skip(SkipReason::CheckFailed("offline".into())),
			false,
		);
		let (text, attention) = status_text(&failed, UpdateMode::Manual);

		assert!(attention);
		assert!(!text.to_lowercase().contains("up to date"), "{text}");
	}

	#[test]
	fn manual_mode_offers_the_update_without_promising_to_install_it() {
		let available = status(
			UpdateState::Available,
			Decision::Skip(SkipReason::ModeManual),
			true,
		);

		assert_eq!(
			status_text(&available, UpdateMode::Manual),
			("Update available: v2".into(), false)
		);
	}

	#[test]
	fn automatic_modes_explain_what_will_happen() {
		let apply = status(UpdateState::Available, Decision::Apply, true);
		assert!(
			status_text(&apply, UpdateMode::OnLaunch)
				.0
				.contains("install automatically")
		);

		let young = status(
			UpdateState::Available,
			Decision::Skip(SkipReason::MinReleaseAgeNotMet { eligible_at: 5 }),
			true,
		);
		assert_eq!(
			status_text(&young, UpdateMode::Daemon).0,
			"Waiting for minimum age: v2"
		);

		let paused = status(
			UpdateState::Available,
			Decision::Skip(SkipReason::AutoPaused),
			true,
		);
		assert!(
			status_text(&paused, UpdateMode::OnLaunch)
				.0
				.contains("paused")
		);

		let rebuilt = status(
			UpdateState::Available,
			Decision::Skip(SkipReason::RebuiltArtifact),
			true,
		);
		let (text, attention) = status_text(&rebuilt, UpdateMode::OnLaunch);
		assert!(attention && text.starts_with("Needs your attention"));
	}

	#[test]
	fn pinned_and_current_apps_say_so() {
		assert_eq!(
			status_text(
				&status(
					UpdateState::Pinned,
					Decision::Skip(SkipReason::Pinned),
					false
				),
				UpdateMode::Manual
			)
			.0,
			"Pinned"
		);
		assert_eq!(
			status_text(
				&status(
					UpdateState::UpToDate,
					Decision::Skip(SkipReason::NotNewer),
					false
				),
				UpdateMode::Manual
			)
			.0,
			"Up to date"
		);
	}
}
