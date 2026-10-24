//! Made-up data for looking at the interface.
//!
//! Set `REMADE_DEMO=1` and the app shows these invented apps in place of real
//! ones. Everything here lives in memory: nothing is read from or written to
//! the real database, and no action in demo mode reaches the engine. The app
//! names are fictional.

use std::collections::HashMap;

use launcher_catalog::{ProjectKind, State};
use launcher_core::{
	AutoStatus, Availability, AvailableUpdate, Decision, ExploreRow, InstalledApp,
	OperationSummary, SkipReason, UpdateState, UpdateStatus,
};
use launcher_model::{InstallationId, MinimumAge, UpdateMode};
use launcher_storage::AppPolicy;

/// True when the person asked for demo data.
pub(crate) fn enabled() -> bool {
	std::env::var_os("REMADE_DEMO").is_some_and(|value| !value.is_empty() && value != "0")
}

pub(crate) struct Demo {
	pub(crate) installed: Vec<InstalledApp>,
	pub(crate) statuses: HashMap<InstallationId, UpdateStatus>,
	pub(crate) policies: HashMap<InstallationId, AppPolicy>,
	pub(crate) history: Vec<OperationSummary>,
	pub(crate) catalog: Vec<ExploreRow>,
}

impl Demo {
	pub(crate) fn new(now: i64) -> Self {
		let apps = [
			(
				"Lumen Notes",
				"example/lumen-notes",
				"v2.4.1",
				true,
				Some("lumen-notes"),
			),
			(
				"Orbit Terminal",
				"example/orbit-terminal",
				"v0.18.0",
				true,
				Some("orbit-terminal"),
			),
			(
				"Fathom Player",
				"example/fathom-player",
				"v5.2.0",
				true,
				Some("fathom-player"),
			),
			("Quill", "example/quill", "v1.9.3", true, Some("quill")),
			("hex-peek", "someone/hex-peek", "v0.3.0", false, None),
		];

		let installed: Vec<InstalledApp> = apps
			.iter()
			.enumerate()
			.map(
				|(index, (name, repository, version, curated, slug))| InstalledApp {
					id: InstallationId::new(),
					name: (*name).to_owned(),
					repository: (*repository).to_owned(),
					version: (*version).to_owned(),
					curated: *curated,
					catalog_slug: slug.map(str::to_owned),
					root: format!("/demo/apps/{index}"),
					launchable: index != 3,
					evidence: "local-observed".into(),
					installed_at: now - 86_400 * (index as i64 + 1),
				},
			)
			.collect();

		let mut statuses = HashMap::new();
		let mut policies = HashMap::new();

		for (app, state) in installed.iter().zip(demo_states(now)) {
			statuses.insert(app.id, status(app, state));
		}

		// Fathom Player is pinned; the status above already says so.
		policies.insert(
			installed[2].id,
			AppPolicy {
				pinned_release_id: Some(1),
				..AppPolicy::default()
			},
		);

		let history = vec![
			entry(
				"update",
				"completed",
				&installed[0],
				"Updated to v2.4.1",
				now - 3_600 * 5,
			),
			entry(
				"install",
				"completed",
				&installed[4],
				"Installed v0.3.0",
				now - 86_400,
			),
			entry(
				"update",
				"failed",
				&installed[1],
				"The download did not match its checksum.",
				now - 86_400 * 2,
			),
			entry(
				"install",
				"completed",
				&installed[3],
				"Installed v1.9.3",
				now - 86_400 * 4,
			),
		];

		Self {
			installed,
			statuses,
			policies,
			history,
			catalog: catalog(),
		}
	}
}

/// What each app's update state is, in the same order as the apps.
fn demo_states(now: i64) -> Vec<(UpdateState, Option<AvailableUpdate>)> {
	let update = |version: &str, size: u64, hours: i64| AvailableUpdate {
		version: version.into(),
		file: format!("app-{version}.zip"),
		size,
		published_at: Some(now - 3_600 * hours),
		rebuilt: false,
	};

	vec![
		(
			UpdateState::Available,
			Some(update("v2.5.0", 48_300_000, 30)),
		),
		(
			UpdateState::CheckFailed("the network did not answer".into()),
			None,
		),
		(UpdateState::Pinned, Some(update("v6.0.0", 92_100_000, 400))),
		(
			UpdateState::Available,
			Some(update("v1.10.0", 12_700_000, 2)),
		),
		(UpdateState::UpToDate, None),
	]
}

fn status(
	app: &InstalledApp,
	(state, available): (UpdateState, Option<AvailableUpdate>),
) -> UpdateStatus {
	let decision = match (&state, &available) {
		(UpdateState::Pinned, _) => Decision::Skip(SkipReason::Pinned),
		(UpdateState::Available, Some(_)) => Decision::Skip(SkipReason::ModeManual),
		_ => Decision::Skip(SkipReason::NotNewer),
	};

	UpdateStatus {
		installation: app.id,
		name: app.name.clone(),
		catalog_slug: app.catalog_slug.clone(),
		installed_version: app.version.clone(),
		state,
		available,
		auto: AutoStatus {
			mode: UpdateMode::Manual,
			minimum_age: MinimumAge::DEFAULT,
			age_is_per_app: false,
			eligible_at: None,
			decision,
		},
	}
}

fn entry(kind: &str, state: &str, app: &InstalledApp, detail: &str, at: i64) -> OperationSummary {
	OperationSummary {
		id: format!("demo-{at}"),
		kind: kind.into(),
		state: state.into(),
		app: Some(app.repository.clone()),
		detail: Some(detail.into()),
		started_at: at,
	}
}

fn catalog() -> Vec<ExploreRow> {
	let row = |slug: &str,
	           name: &str,
	           summary: &str,
	           repository: &str,
	           kind,
	           categories: &[&str],
	           availability| ExploreRow {
		slug: slug.into(),
		name: name.into(),
		summary: summary.into(),
		kind,
		status: State::Active,
		repository: repository.into(),
		categories: categories
			.iter()
			.map(|category| (*category).to_owned())
			.collect(),
		availability,
	};

	vec![
		row(
			"lumen-notes",
			"Lumen Notes",
			"Fast Markdown notes that stay in plain files.",
			"example/lumen-notes",
			ProjectKind::Desktop,
			&["Writing", "Productivity"],
			Availability::ThisDevice,
		),
		row(
			"orbit-terminal",
			"Orbit Terminal",
			"A GPU-accelerated terminal with tabs and splits.",
			"example/orbit-terminal",
			ProjectKind::Desktop,
			&["Developer tools"],
			Availability::ThisDevice,
		),
		row(
			"fathom-player",
			"Fathom Player",
			"A music player for large local libraries.",
			"example/fathom-player",
			ProjectKind::Desktop,
			&["Audio"],
			Availability::ThisDevice,
		),
		row(
			"quill",
			"Quill",
			"A distraction-free editor for long-form writing.",
			"example/quill",
			ProjectKind::Desktop,
			&["Writing"],
			Availability::ThisDevice,
		),
		row(
			"pixelforge",
			"Pixelforge",
			"Pixel art and sprite animation in one window.",
			"example/pixelforge",
			ProjectKind::Desktop,
			&["Graphics"],
			Availability::ThisDevice,
		),
		row(
			"tidepool",
			"Tidepool",
			"Command-line RSS reader with offline sync.",
			"example/tidepool",
			ProjectKind::Cli,
			&["News"],
			Availability::ThisDevice,
		),
		row(
			"ferrite-db",
			"Ferrite DB",
			"A small embedded database server.",
			"example/ferrite-db",
			ProjectKind::Service,
			&["Developer tools"],
			Availability::NeedsNewerLauncher,
		),
		row(
			"mosaic",
			"Mosaic",
			"Tile photos into collages. No packages for this device yet.",
			"example/mosaic",
			ProjectKind::Desktop,
			&["Graphics"],
			Availability::NoPackages,
		),
	]
}

/// Release notes for demo apps.
pub(crate) fn notes(app: &str, version: &str) -> String {
	format!(
		"Demo notes for {app} {version}.\n\n• Faster startup\n• Fixes a crash when opening large files\n• Updated translations\n\nThese notes are made up."
	)
}
