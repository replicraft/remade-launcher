//! Remade Launcher: the official launcher for unofficial apps.

mod ui;

use std::process::ExitCode;
use std::sync::Arc;

use launcher_core::{DataDirectory, Engine};
use tracing_subscriber::EnvFilter;

#[global_allocator]
static ALLOCATOR: mimalloc::MiMalloc = mimalloc::MiMalloc;

fn main() -> ExitCode {
	tracing_subscriber::fmt()
		.with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
		.init();

	match run() {
		Ok(()) => ExitCode::SUCCESS,
		Err(message) => {
			tracing::error!("{message}");
			ExitCode::FAILURE
		}
	}
}

fn run() -> Result<(), String> {
	// The interface owns the main thread, so the engine gets its own small
	// runtime. Two workers are plenty: heavy work will move to dedicated
	// pools, not to more async threads.
	let runtime = tokio::runtime::Builder::new_multi_thread()
		.worker_threads(2)
		.thread_name("remade-async")
		.enable_all()
		.build()
		.map_err(|error| format!("could not start the async runtime: {error}"))?;

	// `REMADE_DATA_DIR` points both programs at another folder, for testing
	// and for keeping several profiles apart.
	let data = match std::env::var_os("REMADE_DATA_DIR") {
		Some(path) if !path.is_empty() => DataDirectory::at(path),
		_ => DataDirectory::platform_default().map_err(|error| error.to_string())?,
	};
	let engine = runtime
		.block_on(Engine::open(&data))
		.map(Arc::new)
		.map_err(|error| error.to_string())?;

	ui::run(Arc::clone(&engine), runtime.handle().clone())
		.map_err(|error| format!("the window failed: {error}"))?;

	// The window is gone, so the interface has dropped its engine handle.
	if let Some(engine) = Arc::into_inner(engine) {
		runtime.block_on(engine.shutdown());
	}

	Ok(())
}
