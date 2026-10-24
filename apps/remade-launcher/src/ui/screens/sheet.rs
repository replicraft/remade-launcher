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

				if dialog_ok(ui, "OK") {
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

				if dialog_ok(ui, "OK") {
					outcome = Outcome::Close;
				}
			}
			Sheet::ConfirmUninstall(app) => {
				ui.heading(format!("Uninstall {}?", app.name));
				ui.add_space(8.0);

				ui.horizontal(|ui| {
					app_tile_for(ui, app.catalog_slug.as_deref(), &app.name, 44.0);

					ui.vertical(|ui| {
						ui.spacing_mut().item_spacing.y = 1.0;
						ui.label(RichText::new(&app.name).family(theme::semibold()));
						ui.label(RichText::new(format!("{} · {}", app.version, app.repository)).small().color(theme::muted(ui)));
						ui.label(RichText::new(format!("Installed {}", launcher_model::local_time::date(app.installed_at))).small().color(theme::muted(ui)));
					});
				});

				ui.add_space(10.0);
				ui.label(RichText::new("Where it lives").small().color(theme::muted(ui)));
				ui.label(RichText::new(&app.root).monospace().small());
				ui.add_space(10.0);

				ui.label("What happens:");
				ui.add_space(2.0);
				ui.label("•  The files the launcher installed are removed.");
				ui.label("•  Anything you edited, and data the app saved itself, is kept.");
				ui.label("•  Afterwards you see exactly what was removed and what stayed.");

				ui.add_space(14.0);

				match dialog_buttons(ui, &Confirm { label: "Uninstall", destructive: true, enabled: true }) {
					Choice::Pending => {}
					Choice::Cancel => outcome = Outcome::Close,
					Choice::Confirm => outcome = Outcome::Uninstall(app.clone()),
				}
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

				ui.add_space(14.0);

				match dialog_buttons(ui, &Confirm { label: "Turn on", destructive: false, enabled: *acknowledged }) {
					Choice::Pending => {}
					Choice::Cancel => outcome = Outcome::Close,
					Choice::Confirm => outcome = Outcome::SetMode { mode: *mode, acknowledged: *acknowledged },
				}
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

				ui.add_space(14.0);

				match dialog_buttons(ui, &Confirm { label: "Update all", destructive: false, enabled: true }) {
					Choice::Pending => {}
					Choice::Cancel => outcome = Outcome::Close,
					Choice::Confirm => outcome = Outcome::UpdateAll(std::mem::take(updates)),
				}
			}
			Sheet::Notes { app, version, text } => {
				ui.heading(format!("{app} {version}"));

				egui::ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
					ui.label(if text.trim().is_empty() { "This release has no notes." } else { text.as_str() });
				});

				if dialog_ok(ui, "Close") {
					outcome = Outcome::Close;
				}
			}
			Sheet::ConfirmRestore => {
				heading_with_icon(ui, icons::WARNING, "Restore defaults?");
				ui.add_space(6.0);

				ui.label("This puts the launcher's own settings back the way a new install has them:");
				ui.add_space(4.0);

				for line in [
					"Appearance follows your system again.",
					"Updates go back to manual, with a one-hour minimum age.",
					"Every app's own update settings are cleared, including pins.",
					"A background updater, if you turned one on, is switched off.",
				] {
					ui.label(format!("•  {line}"));
				}

				ui.add_space(8.0);
				ui.label(RichText::new("Your installed apps and their files are not touched. This cannot be undone.").color(theme::muted(ui)));
				ui.add_space(14.0);

				match dialog_buttons(ui, &Confirm { label: "Restore defaults", destructive: true, enabled: true }) {
					Choice::Pending => {}
					Choice::Cancel => outcome = Outcome::Close,
					Choice::Confirm => outcome = Outcome::RestoreDefaults,
				}
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

				if dialog_ok(ui, "OK") {
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

	let allowed = !ready.unreviewed || *acknowledged;

	match dialog_buttons(
		ui,
		&Confirm {
			label: verb,
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
