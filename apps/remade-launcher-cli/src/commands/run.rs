use std::process::ExitCode;
use std::sync::Arc;

use launcher_core::Engine;
use launcher_model::LauncherError;

use crate::app_ref;
use crate::cli::RunArgs;

pub(crate) async fn run(engine: &Arc<Engine>, args: &RunArgs) -> Result<ExitCode, LauncherError> {
	let apps = engine.installed().await?;
	let app = app_ref::find(&apps, &args.app)?;

	engine.lifecycle().launch(app.id).await?;
	eprintln!("Started {}.", app.name);

	Ok(ExitCode::SUCCESS)
}
