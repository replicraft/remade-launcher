//! Modal sheets: install review, messages, and uninstall confirmation.
//!
//! The review sheet is where consent happens, so it always shows the source
//! repository beside the Install button and spells out what Remade has and
//! has not checked.

use egui::{Id, Modal, RichText, Ui};
use launcher_core::{InstalledApp, OperatorUpdate, Ready, RemovalReport};
use launcher_model::{InstallationId, UpdateMode};
use launcher_resolver::{Candidate, EvidenceLevel};

use crate::ui::components::danger_button;
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
		acknowledged: bool,
	},
	UpdateAll(Vec<OperatorUpdate>),
	RestoreDefaults,
}

pub(crate) fn show(ctx: &egui::Context, sheet: &mut Sheet) -> Outcome {
	let mut outcome = Outcome::Stay;

	let response = Modal::new(Id::unique("sheet")).show(ctx, |ui| {
		ui.set_width(460.0);

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

				if ui.button("OK").clicked() {
					outcome = Outcome::Close;
				}
			}
			Sheet::Review { ready, update_of, acknowledged } => {
				if let Some(next) = review(ui, ready, update_of, acknowledged) {
					outcome = next;
				}
			}
			Sheet::Choose(candidates) => {
				ui.heading("Several packages fit");
				ui.label("More than one package fits this device equally well, and the launcher will not guess which you want. This is a gap in the catalog entry, not something you did.");

				for candidate in candidates.iter() {
					ui.label(format!("• {} ({})", candidate.asset.name, candidate.variant_id));
				}

				if ui.button("OK").clicked() {
					outcome = Outcome::Close;
				}
			}
			Sheet::ConfirmUninstall(app) => {
				ui.heading(format!("Uninstall {}?", app.name));
				ui.label("The launcher removes the files it installed. Files the app saved in its own folder, and any file you edited, are kept and listed afterwards.");

				ui.horizontal(|ui| {
					if ui.button("Cancel").clicked() {
						outcome = Outcome::Close;
					}

					if ui.button("Uninstall").clicked() {
						outcome = Outcome::Uninstall(app.clone());
					}
				});
			}
			Sheet::ConfirmMode { mode, acknowledged } => {
				ui.heading(mode.label());
				ui.label(mode.description());
				ui.add_space(6.0);
				ui.label("Before you turn this on, know that:");
				ui.label("• A new version can change how an app behaves, or stop it working.");
				ui.label("• If a publisher's account is compromised, a harmful release could be installed.");
				ui.label("• An update can migrate an app's data in a way older versions cannot read.");
				ui.label("• Updates run without asking, and no scan can promise software is safe.");

				if *mode == UpdateMode::Daemon {
					ui.add_space(4.0);
					ui.colored_label(
						ui.visuals().warn_fg_color,
						"This also registers a background program that starts each time you sign in, and keeps running after you close the Launcher. You can turn it off here or in your system's login items.",
					);
				}

				ui.add_space(6.0);
				ui.checkbox(acknowledged, "I understand, and I want automatic updates");

				ui.horizontal(|ui| {
					if ui.button("Cancel").clicked() {
						outcome = Outcome::Close;
					}

					if ui.add_enabled(*acknowledged, egui::Button::new("Turn on")).clicked() {
						outcome = Outcome::SetMode { mode: *mode, acknowledged: *acknowledged };
					}
				});
			}
			Sheet::ConfirmUpdateAll(updates) => {
				ui.heading("Update everything?");

				for update in updates.iter() {
					let to = update
						.status
						.available
						.as_ref()
						.map_or("?", |available| available.version.as_str());

					ui.label(format!("• {}: {} to {to}", update.status.name, update.status.installed_version));
				}

				ui.add_space(6.0);
				ui.label(RichText::new("Each app is checked and replaced one at a time. A failure in one does not stop the others.").small().color(theme::muted(ui)));

				ui.horizontal(|ui| {
					if ui.button("Cancel").clicked() {
						outcome = Outcome::Close;
					}

					if ui.button("Update all").clicked() {
						outcome = Outcome::UpdateAll(std::mem::take(updates));
					}
				});
			}
			Sheet::Notes { app, version, text } => {
				ui.heading(format!("{app} {version}"));

				egui::ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
					ui.label(if text.trim().is_empty() { "This release has no notes." } else { text.as_str() });
				});

				if ui.button("Close").clicked() {
					outcome = Outcome::Close;
				}
			}
			Sheet::ConfirmRestore => {
				ui.heading("Restore defaults?");
				ui.label("Appearance goes back to the system setting, updates go back to manual with a one-hour minimum age, and every app's own update settings are cleared. A background updater is turned off.");
				ui.label(RichText::new("Your installed apps and their files are not touched. This cannot be undone.").color(theme::muted(ui)));

				ui.horizontal(|ui| {
					if ui.button("Cancel").clicked() {
						outcome = Outcome::Close;
					}

					if danger_button(ui, "Restore defaults").clicked() {
						outcome = Outcome::RestoreDefaults;
					}
				});
			}
			Sheet::Removed { name, report } => {
				ui.heading(format!("{name} was uninstalled"));
				ui.label(format!("{} files removed.", report.removed_files));

				if !report.is_clean() {
					ui.add_space(6.0);
					ui.label("Some files were kept so nothing you made is lost:");

					for file in report.kept_modified.iter().chain(&report.kept_unknown).take(8) {
						ui.label(RichText::new(format!("• {file}")).color(theme::muted(ui)));
					}

					if let Some(folder) = &report.leftover_folder {
						ui.label(RichText::new(folder).small().color(theme::muted(ui)));
					}
				}

				if ui.button("OK").clicked() {
					outcome = Outcome::Close;
				}
			}
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
	let verb = if update_of.is_some() {
		"Update"
	} else {
		"Install"
	};
	let mut outcome = None;

	ui.heading(format!("{verb} {}", ready.meta.display_name));
	ui.add_space(4.0);

	// The signed or looked-up repository identity, never a description.
	ui.horizontal(|ui| {
		ui.label(RichText::new("Source").color(theme::muted(ui)));
		ui.label(RichText::new(&ready.source_display).family(crate::ui::theme::semibold()));
	});

	if ready.unreviewed {
		ui.colored_label(
			ui.visuals().warn_fg_color,
			"Remade has not reviewed this source. Only install software from people you trust.",
		);
	} else {
		ui.label(RichText::new("Listed in the verified Remade catalog.").color(theme::muted(ui)));
	}

	ui.add_space(6.0);
	fact(ui, "Version", &body.release.tag);
	fact(
		ui,
		"File",
		&format!("{} ({})", body.asset.name, bytes(body.asset.size)),
	);
	fact(ui, "Checked against", evidence(body.install.evidence));

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
			RichText::new(format!(
				"Newer release {} skipped: {}",
				skipped.tag, skipped.reason
			))
			.small()
			.color(theme::muted(ui)),
		);
	}

	if ready.unreviewed {
		ui.add_space(6.0);
		ui.checkbox(
			acknowledged,
			"I understand this source is not reviewed by Remade",
		);
	}

	ui.add_space(8.0);

	ui.horizontal(|ui| {
		if ui.button("Cancel").clicked() {
			outcome = Some(Outcome::Close);
		}

		let allowed = !ready.unreviewed || *acknowledged;

		if ui.add_enabled(allowed, egui::Button::new(verb)).clicked() {
			outcome = Some(Outcome::Install {
				ready: Box::new(ready.clone()),
				update_of: *update_of,
				acknowledged: *acknowledged,
			});
		}
	});

	outcome
}

fn fact(ui: &mut Ui, label: &str, value: &str) {
	ui.horizontal_wrapped(|ui| {
		ui.label(RichText::new(label).color(theme::muted(ui)));
		ui.label(value);
	});
}

fn evidence(level: EvidenceLevel) -> &'static str {
	match level {
		EvidenceLevel::HostDigest => {
			"the checksum GitHub reports for the file. It shows the file is intact, not who made it"
		}
		EvidenceLevel::PinnedSha256 => "a checksum pinned in the signed Remade catalog",
		EvidenceLevel::PublisherSignature => "the publisher's own signature",
		EvidenceLevel::LocalObserved => {
			"nothing beyond the download itself. No checksum was published to compare"
		}
	}
}

/// A UTC date and time for display.
pub(crate) fn when(unix: i64) -> String {
	let time =
		time::OffsetDateTime::from_unix_timestamp(unix).unwrap_or(time::OffsetDateTime::UNIX_EPOCH);

	format!(
		"{}-{:02}-{:02} {:02}:{:02} UTC",
		time.year(),
		u8::from(time.month()),
		time.day(),
		time.hour(),
		time.minute()
	)
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
