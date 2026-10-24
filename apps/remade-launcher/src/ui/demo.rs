//! Made-up data for looking at the interface.
//!
//! Set `REMADE_DEMO=1` and the app shows these invented apps in place of real
//! ones. Everything here lives in memory: nothing is read from or written to
//! the real database, and no action in demo mode reaches the engine. The app
//! names are fictional.

use std::collections::HashMap;

use launcher_catalog::{Maturity, ProjectKind, State};
use launcher_core::{
	AutoStatus, Availability, AvailableUpdate, Decision, ExploreRow, InstalledApp,
	OperationSummary, SkipReason, UpdateState, UpdateStatus,
};
use launcher_model::{InstallationId, MinimumAge, Os, UpdateMode};

use super::icons;
use launcher_storage::AppPolicy;

/// True when the person asked for demo data. Read once.
pub(crate) fn enabled() -> bool {
	static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();

	*ENABLED.get_or_init(|| {
		std::env::var_os("REMADE_DEMO").is_some_and(|value| !value.is_empty() && value != "0")
	})
}

/// An icon for a demo app, standing in for the image a real catalog entry
/// would link to.
pub(crate) fn glyph(slug: &str) -> Option<&'static str> {
	if !enabled() {
		return None;
	}

	Some(match slug {
		"lumen-notes" => icons::NOTE_PENCIL,
		"orbit-terminal" => icons::TERMINAL_WINDOW,
		"fathom-player" => icons::MUSIC_NOTES,
		"quill" => icons::PENCIL_LINE,
		"pixelforge" => icons::PAINT_BRUSH,
		"tidepool" => icons::NEWSPAPER,
		"cinder" => icons::GAUGE,
		"harbor" => icons::HARD_DRIVES,
		"ferrite-db" => icons::TERMINAL,
		"mosaic" => icons::IMAGE,
		_ => return None,
	})
}

/// How finished each demo app claims to be.
fn maturity(slug: &str) -> Maturity {
	match slug {
		"lumen-notes" | "fathom-player" | "orbit-terminal" => Maturity::Stable,
		"quill" | "harbor" | "tidepool" => Maturity::Beta,
		"pixelforge" | "cinder" => Maturity::Alpha,
		_ => Maturity::Experimental,
	}
}

/// Languages and platforms for the demo apps.
fn tech(slug: &str) -> (&'static [&'static str], &'static [Os]) {
	use Os::{Linux, Macos, Windows};

	match slug {
		"lumen-notes" => (&["Rust", "TypeScript"], &[Windows, Macos, Linux]),
		"orbit-terminal" => (&["Rust"], &[Macos, Linux]),
		"fathom-player" => (&["C++"], &[Windows, Macos, Linux]),
		"quill" => (&["Swift"], &[Macos]),
		"pixelforge" => (&["C#"], &[Windows, Linux]),
		"tidepool" => (&["Go"], &[Windows, Macos, Linux]),
		"cinder" => (&["Swift"], &[Macos]),
		"harbor" => (&["Python"], &[Windows, Macos, Linux]),
		"ferrite-db" => (&["Rust", "C"], &[Linux]),
		_ => (&["Python", "C"], &[Windows]),
	}
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
			catalog: catalog(now),
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

fn catalog(now: i64) -> Vec<ExploreRow> {
	let day = 86_400;
	let row = |slug: &str,
	           name: &str,
	           summary: &str,
	           kind,
	           categories: &[&str],
	           availability,
	           featured: bool,
	           days_ago: Option<i64>| ExploreRow {
		slug: slug.into(),
		name: name.into(),
		summary: summary.into(),
		kind,
		status: State::Active,
		repository: format!("example/{slug}"),
		categories: categories
			.iter()
			.map(|category| (*category).to_owned())
			.collect(),
		availability,
		featured,
		maturity: None,
		languages: Vec::new(),
		platforms: Vec::new(),
		added_at: days_ago.map(|days| now - days * day),
	};
	let device = Availability::ThisDevice;

	let mut rows = vec![
		row(
			"lumen-notes",
			"Lumen Notes",
			"Fast Markdown notes that stay in plain files you own.",
			ProjectKind::Desktop,
			&["writing", "productivity"],
			device.clone(),
			true,
			Some(120),
		),
		row(
			"orbit-terminal",
			"Orbit Terminal",
			"A GPU-accelerated terminal with tabs, splits and ligatures.",
			ProjectKind::Desktop,
			&["developer-tools"],
			device.clone(),
			true,
			Some(200),
		),
		row(
			"fathom-player",
			"Fathom Player",
			"A music player built for very large local libraries.",
			ProjectKind::Desktop,
			&["audio"],
			device.clone(),
			true,
			Some(300),
		),
		row(
			"quill",
			"Quill",
			"A distraction-free editor for long-form writing.",
			ProjectKind::Desktop,
			&["writing"],
			device.clone(),
			false,
			Some(60),
		),
		row(
			"pixelforge",
			"Pixelforge",
			"Pixel art and sprite animation in one window.",
			ProjectKind::Desktop,
			&["graphics"],
			device.clone(),
			false,
			Some(3),
		),
		row(
			"tidepool",
			"Tidepool",
			"A command-line RSS reader with offline sync.",
			ProjectKind::Cli,
			&["news", "developer-tools"],
			device.clone(),
			false,
			Some(9),
		),
		row(
			"cinder",
			"Cinder",
			"A tiny system monitor that lives in your menu bar.",
			ProjectKind::Desktop,
			&["utilities"],
			device.clone(),
			false,
			Some(1),
		),
		row(
			"harbor",
			"Harbor",
			"Back up folders to any drive on a schedule.",
			ProjectKind::Desktop,
			&["utilities", "productivity"],
			device.clone(),
			true,
			Some(14),
		),
		row(
			"ferrite-db",
			"Ferrite DB",
			"A small embedded database server.",
			ProjectKind::Service,
			&["developer-tools"],
			Availability::NeedsNewerLauncher,
			false,
			Some(30),
		),
		row(
			"mosaic",
			"Mosaic",
			"Tile photos into collages. No packages for this device yet.",
			ProjectKind::Desktop,
			&["graphics"],
			Availability::NoPackages,
			false,
			None,
		),
	];

	for row in &mut rows {
		let (languages, platforms) = tech(&row.slug);

		row.languages = languages
			.iter()
			.map(|language| (*language).to_owned())
			.collect();
		row.platforms = platforms.to_vec();
		row.maturity = Some(maturity(&row.slug));
	}

	rows
}

/// Release notes for demo apps.
pub(crate) fn notes(app: &str, version: &str) -> String {
	format!(
		"Demo notes for {app} {version}.\n\n• Faster startup\n• Fixes a crash when opening large files\n• Updated translations\n\nThese notes are made up."
	)
}

/// A long description for a demo app. Made up, and the same shape as a real
/// one: a few paragraphs separated by blank lines.
pub(crate) fn description(name: &str) -> String {
	format!(
		"{name} is a made-up app, here so the interface has something to show. Its description is long on purpose: a real listing can run to several paragraphs, and this page should read well when it does.\n\n\
		It opens quickly, keeps your files in ordinary folders you can back up or move, and never asks you to sign in. Everything it does happens on your own computer.\n\n\
		Updates arrive as separate releases. The launcher checks each one before installing it, and you can pin the version you have if a newer one is not for you.\n\n\
		This text is invented. If you are reading it in the launcher, you are looking at demo data."
	)
}

/// A status for an app that has just been installed or updated.
pub(crate) fn up_to_date(app: &InstalledApp) -> UpdateStatus {
	status(app, (UpdateState::UpToDate, None))
}

/// About how big a demo download is, so each one takes a believable time.
pub(crate) fn download_size(slug: &str) -> u64 {
	let megabytes = 24 + (slug.bytes().map(u64::from).sum::<u64>() % 60);

	megabytes * 1_000_000
}
