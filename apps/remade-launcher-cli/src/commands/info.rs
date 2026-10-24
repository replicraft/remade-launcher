use std::process::ExitCode;
use std::sync::Arc;

use launcher_core::{Engine, Project, State, VerificationLevel};
use launcher_model::{ErrorCode, HostTarget, LauncherError};

use super::ensure_catalog;
use crate::cli::{Cli, InfoArgs};
use crate::output::refuse;

pub(crate) async fn run(
	engine: &Arc<Engine>,
	cli: &Cli,
	args: &InfoArgs,
) -> Result<ExitCode, LauncherError> {
	let state = ensure_catalog(engine, cli.offline).await?;
	let catalog = state
		.catalog
		.as_ref()
		.ok_or_else(|| LauncherError::new(ErrorCode::InstallationNotFound, "No catalog."))?;

	let Some(project) = catalog.project(&args.slug) else {
		return Ok(refuse(&format!(
			"\"{}\" is not in the catalog. Try `remade search`.",
			args.slug
		)));
	};

	print_project(&args.slug, project);

	Ok(ExitCode::SUCCESS)
}

fn print_project(slug: &str, project: &Project) {
	let host = HostTarget::current();
	let display = &project.display;

	println!("{} ({slug})", display.name);
	println!("{}", display.summary);
	println!();
	println!("  Source:     {}", project.source.display_name());
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

	let links = &display.links;

	for (label, url) in [
		("Homepage", &links.homepage),
		("Docs", &links.documentation),
		("Source", &links.source),
		("Changelog", &links.changelog),
	] {
		if let Some(url) = url {
			println!("  {:<11} {url}", format!("{label}:"));
		}
	}

	println!();

	if project.variants.is_empty() {
		println!("  No managed package.");

		return;
	}

	println!("  Packages:");

	for variant in &project.variants {
		let here = host.is_some_and(|host| host.runs(variant.os, variant.arch));
		let evidence = match variant.verification.minimum {
			VerificationLevel::HostDigest => "checked against GitHub's checksum",
			VerificationLevel::PinnedSha256 => "checked against a checksum pinned in the catalog",
			VerificationLevel::PublisherSignature => "needs the publisher's signature",
		};

		println!(
			"    {} {} · {}{}",
			variant.os.label(),
			variant.arch.label(),
			variant.format.label(),
			if here { "  (this device)" } else { "" }
		);
		println!("      {evidence}");
	}
}
