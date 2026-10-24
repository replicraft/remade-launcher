//! Modal sheets: install review, messages, and uninstall confirmation.
//!
//! The review sheet is where consent happens, so it always shows the source
//! repository beside the Install button and spells out what Remade has and
//! has not checked.

use egui::{Id, Modal, RichText, Ui};
use launcher_core::{InstalledApp, OperatorUpdate, Ready, RemovalReport};
use launcher_model::{InstallationId, UpdateMode};
use launcher_resolver::{Candidate, EvidenceLevel};

use crate::ui::components::{
	Choice, Confirm, app_tile_for, dialog_buttons, dialog_close, dialog_ok,
};
use crate::ui::i18n::text_for;
use crate::ui::icons;
use crate::ui::theme;

pub(crate) enum Sheet {
	Working(String),
	Message {
		title: String,
		lines: Vec<String>,
	},
	Review {
		ready: Box<Ready>,
		update_of: Option<InstallationId>,
		acknowledged: bool,
	},
	Choose(Vec<Candidate>),
	ConfirmUninstall(InstalledApp),
	/// Turning on an automatic update mode. Never pre-checked.
	ConfirmMode {
		mode: UpdateMode,
		/// One app's own level, or all apps when empty.
		app: Option<InstallationId>,
		acknowledged: bool,
	},
	Removed {
		name: String,
		report: RemovalReport,
	},
	/// Several updates, reviewed together.
	ConfirmUpdateAll(Vec<OperatorUpdate>),
	Notes {
		app: String,
		version: String,
		text: String,
	},
	ConfirmRestore,
}

pub(crate) enum Outcome {
	Stay,
	Close,
	Install {
		ready: Box<Ready>,
		update_of: Option<InstallationId>,
		acknowledged: bool,
	},
	Uninstall(InstalledApp),
	SetMode {
		mode: UpdateMode,
		app: Option<InstallationId>,
		acknowledged: bool,
	},
	UpdateAll(Vec<OperatorUpdate>),
	RestoreDefaults,
}

pub(crate) fn show(ctx: &egui::Context, sheet: &mut Sheet) -> Outcome {
	let mut outcome = Outcome::Stay;

	let response = Modal::new(Id::unique("sheet"))
		.frame(theme::dialog_frame(ctx))
		.show(ctx, |ui| {
			ui.set_width(460.0);

			let origin = ui.cursor().min;
			let working = matches!(sheet, Sheet::Working(_));

			match sheet {
				Sheet::Working(label) => {
					ui.horizontal(|ui| {
						ui.spinner();
						ui.label(label.as_str());
					});
				}
				Sheet::Message { title, lines } => {
					ui.heading(title.as_str());

					for line in lines.iter() {
						ui.label(line.as_str());
					}

					if dialog_ok(ui, &t!("sheet-ok")) {
						outcome = Outcome::Close;
					}
				}
				Sheet::Review {
					ready,
					update_of,
					acknowledged,
				} => {
					if let Some(next) = review(ui, ready, update_of, acknowledged) {
						outcome = next;
					}
				}
				Sheet::Choose(candidates) => {
					ui.heading(t!("sheet-choose-title"));
					ui.label(t!("sheet-choose-body"));

					for candidate in candidates.iter() {
						ui.label(format!(
							"• {} ({})",
							candidate.asset.name, candidate.variant_id
						));
					}

					if dialog_ok(ui, &t!("sheet-ok")) {
						outcome = Outcome::Close;
					}
				}
				Sheet::ConfirmUninstall(app) => {
					ui.heading(t!("sheet-uninstall-title", name = app.name.as_str()));
					ui.add_space(8.0);

					ui.horizontal(|ui| {
						app_tile_for(ui, app.catalog_slug.as_deref(), &app.name, 44.0);

						ui.vertical(|ui| {
							ui.spacing_mut().item_spacing.y = 1.0;
							ui.label(RichText::new(&app.name).family(theme::semibold()));
							ui.label(
								RichText::new(format!("{} · {}", app.version, app.repository))
									.small()
									.color(theme::muted(ui)),
							);
							ui.label(
								RichText::new(t!(
									"sheet-uninstall-installed",
									date = launcher_model::local_time::date(app.installed_at)
								))
								.small()
								.color(theme::muted(ui)),
							);
						});
					});

					ui.add_space(10.0);
					ui.label(
						RichText::new(t!("sheet-uninstall-location"))
							.small()
							.color(theme::muted(ui)),
					);
					ui.label(RichText::new(&app.root).monospace().small());
					ui.add_space(10.0);

					ui.label(t!("sheet-uninstall-what-happens"));
					ui.add_space(2.0);
					ui.label(format!("•  {}", t!("sheet-uninstall-removes")));
					ui.label(format!("•  {}", t!("sheet-uninstall-keeps")));
					ui.label(format!("•  {}", t!("sheet-uninstall-report")));

					ui.add_space(14.0);

					match dialog_buttons(
						ui,
						&Confirm {
							label: &t!("sheet-uninstall-confirm"),
							destructive: true,
							enabled: true,
						},
					) {
						Choice::Pending => {}
						Choice::Cancel => outcome = Outcome::Close,
						Choice::Confirm => outcome = Outcome::Uninstall(app.clone()),
					}
				}
				Sheet::ConfirmMode {
					mode,
					app,
					acknowledged,
				} => {
					ui.heading(text_for("update-mode", mode.as_str()));
					ui.label(text_for(
						"update-mode",
						&format!("{}.description", mode.as_str()),
					));
					ui.add_space(6.0);
					ui.label(t!("sheet-mode-intro"));
					ui.label(format!("• {}", t!("sheet-mode-risk-behavior")));
					ui.label(format!("• {}", t!("sheet-mode-risk-compromised")));
					ui.label(format!("• {}", t!("sheet-mode-risk-migration")));
					ui.label(format!("• {}", t!("sheet-mode-risk-unattended")));

					if *mode == UpdateMode::Daemon && app.is_none() {
						ui.add_space(4.0);
						ui.colored_label(
							ui.visuals().warn_fg_color,
							t!("sheet-mode-daemon-warning"),
						);
					}

					ui.add_space(6.0);
					ui.checkbox(acknowledged, t!("sheet-mode-acknowledge"));

					ui.add_space(14.0);

					match dialog_buttons(
						ui,
						&Confirm {
							label: &t!("sheet-mode-confirm"),
							destructive: false,
							enabled: *acknowledged,
						},
					) {
						Choice::Pending => {}
						Choice::Cancel => outcome = Outcome::Close,
						Choice::Confirm => {
							outcome = Outcome::SetMode {
								mode: *mode,
								app: *app,
								acknowledged: *acknowledged,
							}
						}
					}
				}
				Sheet::ConfirmUpdateAll(updates) => {
					ui.heading(t!("sheet-update-all-title"));

					for update in updates.iter() {
						let to = update
							.status
							.available
							.as_ref()
							.map_or("?", |available| available.version.as_str());

						ui.label(format!(
							"• {}",
							t!(
								"sheet-update-all-line",
								name = update.status.name.as_str(),
								from = update.status.installed_version.as_str(),
								to = to
							)
						));
					}

					ui.add_space(6.0);
					ui.label(
						RichText::new(t!("sheet-update-all-note"))
							.small()
							.color(theme::muted(ui)),
					);

					ui.add_space(14.0);

					match dialog_buttons(
						ui,
						&Confirm {
							label: &t!("sheet-update-all-confirm"),
							destructive: false,
							enabled: true,
						},
					) {
						Choice::Pending => {}
						Choice::Cancel => outcome = Outcome::Close,
						Choice::Confirm => outcome = Outcome::UpdateAll(std::mem::take(updates)),
					}
				}
				Sheet::Notes { app, version, text } => {
					ui.heading(format!("{app} {version}"));

					egui::ScrollArea::vertical()
						.max_height(320.0)
						.show(ui, |ui| {
							ui.label(if text.trim().is_empty() {
								t!("sheet-notes-empty")
							} else {
								text.clone()
							});
						});

					if dialog_ok(ui, &t!("sheet-close")) {
						outcome = Outcome::Close;
					}
				}
				Sheet::ConfirmRestore => {
					heading_with_icon(ui, icons::WARNING, &t!("sheet-restore-title"));
					ui.add_space(6.0);

					ui.label(t!("sheet-restore-intro"));
					ui.add_space(4.0);

					for line in [
						t!("sheet-restore-appearance"),
						t!("sheet-restore-updates"),
						t!("sheet-restore-app-settings"),
						t!("sheet-restore-daemon"),
					] {
						ui.label(format!("•  {line}"));
					}

					ui.add_space(8.0);
					ui.label(RichText::new(t!("sheet-restore-note")).color(theme::muted(ui)));
					ui.add_space(14.0);

					match dialog_buttons(
						ui,
						&Confirm {
							label: &t!("sheet-restore-confirm"),
							destructive: true,
							enabled: true,
						},
					) {
						Choice::Pending => {}
						Choice::Cancel => outcome = Outcome::Close,
						Choice::Confirm => outcome = Outcome::RestoreDefaults,
					}
				}
				Sheet::Removed { name, report } => {
					ui.heading(t!("sheet-removed-title", name = name.as_str()));
					ui.label(t!(
						"sheet-removed-count",
						count = report.removed_files as i64
					));

					if !report.is_clean() {
						ui.add_space(6.0);
						ui.label(t!("sheet-removed-kept"));

						for file in report
							.kept_modified
							.iter()
							.chain(&report.kept_unknown)
							.take(8)
						{
							ui.label(RichText::new(format!("• {file}")).color(theme::muted(ui)));
						}

						if let Some(folder) = &report.leftover_folder {
							ui.label(RichText::new(folder).small().color(theme::muted(ui)));
						}
					}

					if dialog_ok(ui, &t!("sheet-ok")) {
						outcome = Outcome::Close;
					}
				}
			}

			if !working && dialog_close(ui, origin, 460.0) {
				outcome = Outcome::Close;
			}
		});

	// Escape or a click outside dismisses read-only sheets, never a working one.
	if response.should_close()
		&& !matches!(sheet, Sheet::Working(_))
		&& matches!(outcome, Outcome::Stay)
	{
		outcome = Outcome::Close;
	}

	outcome
}

fn review(
	ui: &mut Ui,
	ready: &Ready,
	update_of: &Option<InstallationId>,
	acknowledged: &mut bool,
) -> Option<Outcome> {
	let body = ready.plan.body();
	let updating = update_of.is_some();
	let verb = if updating {
		t!("sheet-review-update")
	} else {
		t!("sheet-review-install")
	};
	let mut outcome = None;

	let name = ready.meta.display_name.as_str();

	ui.heading(if updating {
		t!("sheet-review-update-title", name = name)
	} else {
		t!("sheet-review-install-title", name = name)
	});
	ui.add_space(4.0);

	// The signed or looked-up repository identity, never a description.
	ui.horizontal(|ui| {
		ui.label(RichText::new(t!("sheet-review-source")).color(theme::muted(ui)));
		ui.label(RichText::new(&ready.source_display).family(crate::ui::theme::semibold()));
	});

	if ready.unreviewed {
		ui.colored_label(ui.visuals().warn_fg_color, t!("sheet-review-unreviewed"));
	} else {
		ui.label(RichText::new(t!("sheet-review-verified")).color(theme::muted(ui)));
	}

	ui.add_space(6.0);
	fact(ui, &t!("sheet-review-version"), &body.release.tag);
	fact(
		ui,
		&t!("sheet-review-file"),
		&format!("{} ({})", body.asset.name, bytes(body.asset.size)),
	);
	fact(
		ui,
		&t!("sheet-review-checked-against"),
		&evidence(body.install.evidence),
	);

	if !ready.candidate.explanation.is_empty() {
		ui.add_space(6.0);

		for line in &ready.candidate.explanation {
			ui.label(
				RichText::new(format!("• {line}"))
					.small()
					.color(theme::muted(ui)),
			);
		}
	}

	for note in &ready.notes {
		ui.label(RichText::new(note).small().color(theme::muted(ui)));
	}

	for skipped in &ready.candidate.skipped_newer {
		ui.label(
			RichText::new(t!(
				"sheet-review-skipped",
				tag = skipped.tag.as_str(),
				reason = skipped.reason.as_str()
			))
			.small()
			.color(theme::muted(ui)),
		);
	}

	if ready.unreviewed {
		ui.add_space(6.0);
		ui.checkbox(acknowledged, t!("sheet-review-acknowledge"));
	}

	ui.add_space(8.0);

	let allowed = !ready.unreviewed || *acknowledged;

	match dialog_buttons(
		ui,
		&Confirm {
			label: &verb,
			destructive: false,
			enabled: allowed,
		},
	) {
		Choice::Pending => {}
		Choice::Cancel => outcome = Some(Outcome::Close),
		Choice::Confirm => {
			outcome = Some(Outcome::Install {
				ready: Box::new(ready.clone()),
				update_of: *update_of,
				acknowledged: *acknowledged,
			});
		}
	}

	outcome
}

fn fact(ui: &mut Ui, label: &str, value: &str) {
	ui.horizontal_wrapped(|ui| {
		ui.label(RichText::new(label).color(theme::muted(ui)));
		ui.label(value);
	});
}

fn evidence(level: EvidenceLevel) -> String {
	match level {
		EvidenceLevel::HostDigest => {
			t!("sheet-evidence-host-digest")
		}
		EvidenceLevel::PinnedSha256 => t!("sheet-evidence-pinned"),
		EvidenceLevel::PublisherSignature => t!("sheet-evidence-signature"),
		EvidenceLevel::LocalObserved => {
			t!("sheet-evidence-local")
		}
	}
}

/// A date and time in the person's own time zone.
pub(crate) fn when(unix: i64) -> String {
	launcher_model::local_time::friendly(unix, time::OffsetDateTime::now_utc().unix_timestamp())
}

pub(crate) fn bytes(size: u64) -> String {
	const UNITS: [&str; 4] = ["B", "KB", "MB", "GB"];

	let mut value = size as f64;
	let mut unit = 0;

	while value >= 1024.0 && unit < UNITS.len() - 1 {
		value /= 1024.0;
		unit += 1;
	}

	if unit == 0 {
		format!("{size} B")
	} else {
		format!("{value:.1} {}", UNITS[unit])
	}
}

/// A dialog title with a leading icon.
fn heading_with_icon(ui: &mut Ui, icon: &str, title: &str) {
	ui.horizontal(|ui| {
		ui.label(
			RichText::new(icon)
				.size(26.0)
				.color(theme::palette(ui).danger),
		);
		ui.heading(title);
	});
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn formats_sizes() {
		assert_eq!(bytes(0), "0 B");
		assert_eq!(bytes(1023), "1023 B");
		assert_eq!(bytes(1536), "1.5 KB");
		assert_eq!(bytes(5 * 1024 * 1024), "5.0 MB");
		assert_eq!(bytes(3 * 1024 * 1024 * 1024), "3.0 GB");
	}
}
