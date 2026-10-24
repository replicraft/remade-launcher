//! Read-only commands about the catalog: search, info, platforms, releases,
//! changelog, and trust.

use std::sync::Arc;

use launcher_core::{Availability, Engine, ExploreQuery, ExploreRow, Match, ReleaseStatus, State};
use serde_json::{Value, json};

use super::shared::ensure_catalog;
use crate::cli::{
	ChangelogArgs, ExploreArgs, OptionalSlugArgs, ReleasesArgs, SearchArgs, SlugArgs, TrustAction,
};
use crate::context::Ctx;
use crate::errors::CliError;
use crate::exit::ExitStatus;
use crate::output::{iso, print_table, refuse, truncate, when};

fn availability_word(availability: &Availability) -> &'static str {
	match availability {
		Availability::ThisDevice => "available",
		Availability::NeedsNewerLauncher => "needs_newer_launcher",
		Availability::OtherPlatforms(_) => "other_platforms",
		Availability::NoPackages => "no_package",
	}
}

fn state_word(state: State) -> &'static str {
	match state {
		State::Active => "active",
		State::Paused => "paused",
		State::Revoked => "revoked",
	}
}

async fn catalog_results(
	ctx: &Ctx,
	engine: &Arc<Engine>,
	text: String,
	all: bool,
) -> Result<ExitStatus, CliError> {
	ensure_catalog(ctx, engine).await?;

	let rows = engine
		.explore(ExploreQuery {
			text: text.clone(),
			compatible_only: !all,
		})
		.await;
	let installed: Vec<String> = engine
		.installed()
		.await?
		.into_iter()
		.filter_map(|app| app.catalog_slug)
		.collect();

	if ctx.machine() {
		let results: Vec<Value> = rows
			.iter()
			.map(|row| row_json(row, installed.contains(&row.slug)))
			.collect();

		ctx.emit(json!({ "query": text, "include_other_devices": all, "count": results.len(), "results": results }));

		return Ok(ExitStatus::Success);
	}

	if rows.is_empty() {
		ctx.note(format!(
			"No apps match.{}",
			if all {
				""
			} else {
				" Add --all to include apps for other devices."
			}
		));

		return Ok(ExitStatus::Success);
	}

	if rows.iter().any(|row| row.matched == Match::Close) {
		ctx.note("No exact matches. These are the closest by spelling.");
	}

	let table: Vec<Vec<String>> = rows
		.iter()
		.map(|row| {
			vec![
				row.slug.clone(),
				truncate(&row.name, 28),
				row.repository.clone(),
				match &row.availability {
					Availability::ThisDevice => "yes".to_owned(),
					Availability::NeedsNewerLauncher => "newer launcher".to_owned(),
					Availability::OtherPlatforms(_) => "no".to_owned(),
					Availability::NoPackages => "no package".to_owned(),
				},
				if installed.contains(&row.slug) {
					"installed"
				} else {
					""
				}
				.to_owned(),
				truncate(&row.summary, 50),
			]
		})
		.collect();

	print_table(
		&["APP", "NAME", "SOURCE", "THIS DEVICE", "STATE", "SUMMARY"],
		&table,
	);

	Ok(ExitStatus::Success)
}

fn row_json(row: &ExploreRow, installed: bool) -> Value {
	json!({
		"slug": row.slug,
		"name": row.name,
		"summary": row.summary,
		"kind": row.kind.label(),
		"source": row.repository,
		"status": state_word(row.status),
		"availability": availability_word(&row.availability),
		"compatible_with_this_device": row.availability == Availability::ThisDevice,
		"installed": installed,
		"match": match row.matched {
			Match::Exact => "exact",
			Match::Close => "close",
		},
		"trust": "listed in the verified Remade catalog",
	})
}

pub(crate) async fn search(
	ctx: &Ctx,
	engine: &Arc<Engine>,
	args: &SearchArgs,
) -> Result<ExitStatus, CliError> {
	catalog_results(ctx, engine, args.query.join(" "), args.all).await
}

pub(crate) async fn explore(
	ctx: &Ctx,
	engine: &Arc<Engine>,
	args: &ExploreArgs,
) -> Result<ExitStatus, CliError> {
	catalog_results(ctx, engine, String::new(), args.all).await
}

pub(crate) async fn info(
	ctx: &Ctx,
	engine: &Arc<Engine>,
	args: &SlugArgs,
) -> Result<ExitStatus, CliError> {
	let state = ensure_catalog(ctx, engine).await?;
	let catalog = state
		.catalog
		.clone()
		.ok_or_else(|| CliError::usage("No catalog."))?;

	let Some(project) = catalog.project(&args.slug) else {
		return Ok(refuse(
			ctx,
			"APP_NOT_IN_CATALOG",
			ExitStatus::Usage,
			&format!(
				"\"{}\" is not in the catalog. Try `remade search`.",
				args.slug
			),
		));
	};

	let installed = engine
		.installed()
		.await?
		.into_iter()
		.find(|app| app.catalog_slug.as_deref() == Some(args.slug.as_str()));

	let platforms = engine.platforms(Some(&args.slug))?;
	let variants: Vec<Value> = platforms.apps[0]
		.1
		.iter()
		.map(|variant| {
			json!({
				"id": variant.variant_id,
				"os": variant.os,
				"architecture": variant.arch,
				"format": variant.format,
				"supported_here": variant.supported,
				"reason": variant.reason,
			})
		})
		.collect();

	// The newest eligible release, when we are online and it can be looked up.
	let latest = if ctx.offline {
		None
	} else {
		match engine.releases(&args.slug).await {
			Ok(releases) => releases.into_iter().find(|release| {
				matches!(
					release.status,
					ReleaseStatus::Eligible { has_package: true }
				)
			}),
			Err(error) => {
				ctx.warn(format!(
					"Could not look up releases: {}",
					error.user_message()
				));
				None
			}
		}
	};

	if ctx.machine() {
		ctx.emit(json!({
			"slug": args.slug,
			"name": project.display.name,
			"summary": project.display.summary,
			"kind": project.display.kind.label(),
			"categories": project.display.categories,
			"license": project.display.license,
			"source": {
				"provider": "github",
				"repository": project.source.display_name(),
				"repository_id": project.source.repository_id,
				"owner_id": project.source.owner_id,
			},
			"status": state_word(project.status.state),
			"status_reason": project.status.reason,
			"trust": {
				"origin": "curated",
				"catalog_revision": catalog.revision(),
				"verified_by": catalog.verified_by(),
			},
			"installed": installed.as_ref().map(super::shared::app_json),
			"latest_release": latest.as_ref().map(|release| json!({ "tag": release.tag, "published_at": release.published_at.map(iso) })),
			"packages": variants,
			"links": {
				"homepage": project.display.links.homepage,
				"documentation": project.display.links.documentation,
				"source": project.display.links.source,
				"changelog": project.display.links.changelog,
			},
		}));

		return Ok(ExitStatus::Success);
	}

	let display = &project.display;

	println!("{} ({})", display.name, args.slug);
	println!("{}", display.summary);
	println!();
	println!("  Source:     {}", project.source.display_name());
	println!(
		"  Trust:      listed in the verified Remade catalog (revision {})",
		catalog.revision()
	);
	println!("  Type:       {}", display.kind.label());

	if !display.categories.is_empty() {
		println!("  Categories: {}", display.categories.join(", "));
	}

	if let Some(license) = &display.license {
		println!("  License:    {license}");
	}

	match project.status.state {
		State::Active => {}
		State::Paused => println!("  Status:     paused. New installs are on hold."),
		State::Revoked => println!("  Status:     revoked by Remade."),
	}

	if let Some(reason) = &project.status.reason {
		println!("              {reason}");
	}

	if let Some(app) = &installed {
		println!("  Installed:  {} ({})", app.version, app.id.short());
	}

	if let Some(release) = &latest {
		println!("  Latest:     {}", release.tag);
	}

	for (label, url) in [
		("Homepage", &display.links.homepage),
		("Docs", &display.links.documentation),
		("Source", &display.links.source),
		("Changelog", &display.links.changelog),
	] {
		if let Some(url) = url {
			println!("  {:<11} {url}", format!("{label}:"));
		}
	}

	println!();

	if variants.is_empty() {
		println!("  No managed package.");

		return Ok(ExitStatus::Success);
	}

	println!("  Packages:");

	for variant in &platforms.apps[0].1 {
		println!(
			"    {} {} · {}{}",
			variant.os,
			variant.arch,
			variant.format,
			if variant.supported {
				"  (this device)"
			} else {
				""
			}
		);
		println!("      {}", variant.reason);
	}

	Ok(ExitStatus::Success)
}

pub(crate) async fn platforms(
	ctx: &Ctx,
	engine: &Arc<Engine>,
	args: &OptionalSlugArgs,
) -> Result<ExitStatus, CliError> {
	ensure_catalog(ctx, engine).await?;

	let report = engine.platforms(args.slug.as_deref())?;
	let host = report
		.host
		.map(|host| format!("{} {}", host.os.label(), host.arch.label()));

	if ctx.machine() {
		let apps: Vec<Value> = report
			.apps
			.iter()
			.map(|(slug, variants)| {
				json!({
					"slug": slug,
					"packages": variants.iter().map(|variant| json!({
						"id": variant.variant_id, "os": variant.os, "architecture": variant.arch,
						"format": variant.format, "supported_here": variant.supported, "reason": variant.reason,
					})).collect::<Vec<_>>(),
				})
			})
			.collect();

		ctx.emit(json!({ "this_device": host, "apps": apps }));

		return Ok(ExitStatus::Success);
	}

	println!(
		"This device: {}",
		host.as_deref().unwrap_or("not recognized by the catalog")
	);

	for (slug, variants) in &report.apps {
		println!();
		println!("{slug}");

		for variant in variants {
			println!(
				"  {} {} · {} · {}: {}",
				variant.os,
				variant.arch,
				variant.format,
				if variant.supported {
					"supported"
				} else {
					"not supported"
				},
				variant.reason
			);
		}
	}

	Ok(ExitStatus::Success)
}

pub(crate) async fn releases(
	ctx: &Ctx,
	engine: &Arc<Engine>,
	args: &ReleasesArgs,
) -> Result<ExitStatus, CliError> {
	if ctx.offline {
		return Ok(refuse(
			ctx,
			"OFFLINE",
			ExitStatus::Network,
			"Listing releases needs the network.",
		));
	}

	ensure_catalog(ctx, engine).await?;

	let all = engine.releases(&args.slug).await?;
	let shown: Vec<_> = all.iter().take(args.limit).collect();

	if ctx.machine() {
		let releases: Vec<Value> = shown
			.iter()
			.map(|release| {
				let (eligible, has_package, why) = match &release.status {
					ReleaseStatus::Eligible { has_package } => (true, Some(*has_package), None),
					ReleaseStatus::Excluded(why) => (false, None, Some(*why)),
				};

				json!({
					"tag": release.tag, "prerelease": release.prerelease,
					"published_at": release.published_at.map(iso),
					"eligible": eligible, "has_package_for_this_device": has_package, "excluded_because": why,
				})
			})
			.collect();

		ctx.emit(json!({ "slug": args.slug, "total": all.len(), "releases": releases, "truncated": all.len() > shown.len() }));

		return Ok(ExitStatus::Success);
	}

	let rows: Vec<Vec<String>> = shown
		.iter()
		.map(|release| {
			vec![
				release.tag.clone(),
				release.published_at.map(when).unwrap_or_default(),
				match &release.status {
					ReleaseStatus::Eligible { has_package: true } => "eligible".to_owned(),
					ReleaseStatus::Eligible { has_package: false } => {
						"eligible, no package for this device".to_owned()
					}
					ReleaseStatus::Excluded(why) => format!("skipped: {why}"),
				},
			]
		})
		.collect();

	print_table(&["RELEASE", "PUBLISHED", "VERDICT"], &rows);

	if all.len() > shown.len() {
		ctx.note(format!(
			"{} more. Use -n to see them.",
			all.len() - shown.len()
		));
	}

	Ok(ExitStatus::Success)
}

pub(crate) async fn changelog(
	ctx: &Ctx,
	engine: &Arc<Engine>,
	args: &ChangelogArgs,
) -> Result<ExitStatus, CliError> {
	if ctx.offline {
		return Ok(refuse(
			ctx,
			"OFFLINE",
			ExitStatus::Network,
			"Reading a changelog needs the network.",
		));
	}

	ensure_catalog(ctx, engine).await?;

	let notes = engine
		.changelog(&args.slug, args.release.as_deref())
		.await?;

	if ctx.machine() {
		ctx.emit(json!({ "slug": args.slug, "version": notes.version, "text": notes.text }));
	} else {
		println!("{} {}\n", args.slug, notes.version);
		println!(
			"{}",
			if notes.text.is_empty() {
				"(no release notes)"
			} else {
				&notes.text
			}
		);
	}

	if args.open {
		let state = engine.catalog().state();

		if let Some(project) = state
			.catalog
			.as_ref()
			.and_then(|catalog| catalog.project(&args.slug))
		{
			let url = format!(
				"https://github.com/{}/releases/tag/{}",
				project.source.display_name(),
				notes.version
			);

			if open_in_browser(&url).is_err() {
				ctx.warn(format!("No browser could be opened. The page is {url}"));
			}
		}
	}

	Ok(ExitStatus::Success)
}

pub(crate) async fn trust(
	ctx: &Ctx,
	engine: &Arc<Engine>,
	action: &TrustAction,
) -> Result<ExitStatus, CliError> {
	match action {
		TrustAction::Status | TrustAction::Keys => {
			let status = engine.trust_status();
			let keys_only = matches!(action, TrustAction::Keys);

			if ctx.machine() {
				ctx.emit(json!({
					"feed_id": status.feed_id,
					"trusted_key_ids": status.trusted_key_ids,
					"catalog_revision": status.catalog_revision,
					"verified_by": status.verified_by,
				}));

				return Ok(ExitStatus::Success);
			}

			if keys_only {
				for key in &status.trusted_key_ids {
					println!("{key}");
				}

				if status.trusted_key_ids.is_empty() {
					ctx.note("This build trusts no catalog signing keys yet.");
				}

				return Ok(ExitStatus::Success);
			}

			println!("Feed:           {}", status.feed_id);
			println!(
				"Trusted keys:   {}",
				if status.trusted_key_ids.is_empty() {
					"none".to_owned()
				} else {
					status.trusted_key_ids.join(", ")
				}
			);

			if let Some(revision) = status.catalog_revision {
				println!(
					"Catalog:        revision {revision}, signed by {}",
					status.verified_by.join(", ")
				);
			} else {
				println!("Catalog:        none verified");
			}

			println!();
			println!("Keys are compiled into this build. Nothing downloaded can add one.");
		}
		TrustAction::Explain { slug } => {
			ensure_catalog(ctx, engine).await?;

			let lines = engine.explain_trust(slug)?;

			if ctx.machine() {
				ctx.emit(json!({ "slug": slug, "explanation": lines }));
			} else {
				for line in lines {
					println!("{line}");
				}
			}
		}
	}

	Ok(ExitStatus::Success)
}

/// Whether an app has a package for this device. The exit status says so, so
/// a script can ask without reading output.
pub(crate) async fn compatible(
	ctx: &Ctx,
	engine: &Arc<Engine>,
	args: &SlugArgs,
) -> Result<ExitStatus, CliError> {
	ensure_catalog(ctx, engine).await?;

	let state = engine.catalog().state();

	if state
		.catalog
		.as_ref()
		.and_then(|catalog| catalog.project(&args.slug))
		.is_none()
	{
		return Ok(refuse(
			ctx,
			"APP_NOT_IN_CATALOG",
			ExitStatus::Usage,
			&format!(
				"\"{}\" is not in the catalog. Try `remade search`.",
				args.slug
			),
		));
	}

	let report = engine.platforms(Some(&args.slug))?;
	let variants = report
		.apps
		.first()
		.map(|(_, variants)| variants.as_slice())
		.unwrap_or_default();
	let supported: Vec<_> = variants
		.iter()
		.filter(|variant| variant.supported)
		.collect();

	if ctx.machine() {
		ctx.emit(json!({
			"slug": args.slug,
			"compatible": !supported.is_empty(),
			"packages": supported.iter().map(|variant| json!({ "id": variant.variant_id, "format": variant.format })).collect::<Vec<_>>(),
			"reasons": variants.iter().filter(|variant| !variant.supported).map(|variant| json!({ "id": variant.variant_id, "reason": variant.reason })).collect::<Vec<_>>(),
		}));
	} else if supported.is_empty() {
		println!("{} has no package for this device.", args.slug);

		for variant in variants {
			println!(
				"  {} {} · {}: {}",
				variant.os, variant.arch, variant.format, variant.reason
			);
		}
	} else {
		println!(
			"{} has {} package(s) for this device.",
			args.slug,
			supported.len()
		);
	}

	Ok(if supported.is_empty() {
		ExitStatus::Unsupported
	} else {
		ExitStatus::Success
	})
}

/// Opens `url` in the system browser. The address is always https and always
/// passed as one argument, never through a shell.
fn open_in_browser(url: &str) -> Result<(), CliError> {
	let parsed =
		launcher_net::Url::parse(url).map_err(|_| CliError::usage("That address is not valid."))?;

	if parsed.scheme() != "https" {
		return Err(CliError::usage("Only https addresses are opened."));
	}

	let mut command = if cfg!(target_os = "macos") {
		let mut command = std::process::Command::new("/usr/bin/open");

		command.arg(parsed.as_str());
		command
	} else if cfg!(target_os = "windows") {
		let mut command = std::process::Command::new("rundll32");

		command.args(["url.dll,FileProtocolHandler", parsed.as_str()]);
		command
	} else {
		let mut command = std::process::Command::new("xdg-open");

		command.arg(parsed.as_str());
		command
	};

	command
		.stdin(std::process::Stdio::null())
		.stdout(std::process::Stdio::null())
		.stderr(std::process::Stdio::null())
		.spawn()
		.map(drop)
		.map_err(|_| {
			CliError::new(
				"NO_BROWSER",
				ExitStatus::Unsupported,
				"There is no way to open a browser from here. The address is shown instead.",
			)
		})
}

/// Opens an app's repository or homepage, or prints the address when no
/// browser can be opened.
pub(crate) async fn open(
	ctx: &Ctx,
	engine: &Arc<Engine>,
	args: &crate::cli::OpenArgs,
) -> Result<ExitStatus, CliError> {
	let state = ensure_catalog(ctx, engine).await?;
	let Some(project) = state
		.catalog
		.as_ref()
		.and_then(|catalog| catalog.project(&args.slug))
	else {
		return Ok(refuse(
			ctx,
			"APP_NOT_IN_CATALOG",
			ExitStatus::Usage,
			&format!(
				"\"{}\" is not in the catalog. Try `remade search`.",
				args.slug
			),
		));
	};

	let url = if args.homepage {
		match &project.display.links.homepage {
			Some(url) => url.clone(),
			None => {
				return Ok(refuse(
					ctx,
					"NO_HOMEPAGE",
					ExitStatus::Usage,
					"That app lists no homepage.",
				));
			}
		}
	} else {
		format!("https://github.com/{}", project.source.display_name())
	};

	let opened = open_in_browser(&url);

	if ctx.machine() {
		ctx.emit(json!({ "url": url, "opened": opened.is_ok() }));
	} else {
		println!("{url}");
	}

	match opened {
		Ok(()) => Ok(ExitStatus::Success),
		Err(error) if ctx.machine() => Err(error),
		Err(_) => Ok(ExitStatus::Success),
	}
}
