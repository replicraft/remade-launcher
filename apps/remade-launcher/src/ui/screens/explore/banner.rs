//! What the person needs to know about the catalog itself.

use egui::Ui;
use launcher_core::{CatalogState, Origin, ProblemKind};
use time::OffsetDateTime;

use crate::ui::commands::Commands;
use crate::ui::theme;

/// What the user needs to know about the catalog itself. Network trouble and
/// rejected bytes are worded differently, because they mean different things.
pub(super) fn banner(ui: &mut Ui, state: &CatalogState, commands: &Commands) {
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
	launcher_model::local_time::date(time.unix_timestamp())
}
