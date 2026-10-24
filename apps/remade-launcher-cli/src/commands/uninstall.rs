use std::process::ExitCode;
use std::sync::Arc;

use launcher_core::Engine;
use launcher_model::LauncherError;

use crate::cli::UninstallArgs;
use crate::output::refuse;
use crate::progress::Printer;
use crate::{app_ref, prompt};

pub(crate) async fn run(
	engine: &Arc<Engine>,
	args: &UninstallArgs,
) -> Result<ExitCode, LauncherError> {
	let apps = engine.installed().await?;
	let app = app_ref::find(&apps, &args.app)?;

	eprintln!(
		"This removes the files the launcher installed for {} {}.",
		app.name, app.version
	);
	eprintln!("Files the app saved in its folder, and files you edited, are kept.");

	if !prompt::confirm(&format!("Uninstall {}?", app.name), args.yes)? {
		return Ok(refuse("Cancelled."));
	}

	let printer = Printer::start(engine);
	let now = time::OffsetDateTime::now_utc().unix_timestamp();
	let result = engine.lifecycle().uninstall(app.id, now).await;

	printer.finish().await;

	let report = result?;

	println!(
		"Uninstalled {}. {} files removed.",
		app.name, report.removed_files
	);

	if !report.is_clean() {
		eprintln!("Kept so nothing you made is lost:");

		for file in report
			.kept_modified
			.iter()
			.chain(&report.kept_unknown)
			.take(10)
		{
			eprintln!("  {file}");
		}

		if let Some(folder) = &report.leftover_folder {
			eprintln!("in {folder}");
		}
	}

	Ok(ExitCode::SUCCESS)
}
