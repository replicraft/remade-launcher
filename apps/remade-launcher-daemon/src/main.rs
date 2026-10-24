//! The per-user background updater.
//!
//! It starts at login only if the user turned on the background mode and
//! registered it. It has no window, no tray icon, and no network listener:
//! it wakes on a timer, asks the shared engine to look for updates, and
//! answers a few requests on a socket only its own user can open. It never
//! runs as an administrator.

mod backend;
mod logging;

use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

use launcher_background::{BackgroundPaths, Service, SystemClock, ipc};
use launcher_core::{DataDirectory, Engine};
use launcher_model::UpdateMode;

#[global_allocator]
static ALLOCATOR: mimalloc::MiMalloc = mimalloc::MiMalloc;

fn data_directory() -> Result<DataDirectory, String> {
	let mut arguments = std::env::args().skip(1);

	match arguments.next().as_deref() {
		None => DataDirectory::platform_default().map_err(|error| error.to_string()),
		Some("--data-dir") => arguments
			.next()
			.map(|path| DataDirectory::at(PathBuf::from(path)))
			.ok_or_else(|| "--data-dir needs a folder".to_owned()),
		Some("--version") => {
			println!(
				"remade-launcher-daemon {}",
				launcher_model::product::VERSION
			);

			std::process::exit(0);
		}
		Some(other) => Err(format!(
			"unknown argument {other}. This program takes only --data-dir <folder>."
		)),
	}
}

#[tokio::main(flavor = "multi_thread", worker_threads = 2)]
async fn main() -> ExitCode {
	let data = match data_directory() {
		Ok(data) => data,
		Err(message) => {
			eprintln!("error: {message}");

			return ExitCode::from(2);
		}
	};

	let paths = BackgroundPaths::new(data.root());

	if let Err(message) = logging::start(&paths) {
		eprintln!("error: {message}");

		return ExitCode::FAILURE;
	}

	match run(data, paths).await {
		Ok(()) => ExitCode::SUCCESS,
		Err(message) => {
			tracing::error!("{message}");

			ExitCode::FAILURE
		}
	}
}

async fn run(data: DataDirectory, paths: BackgroundPaths) -> Result<(), String> {
	// One updater per user. A second one finds the lock held and leaves.
	let Some(_lock) = backend::take_lock(&paths) else {
		tracing::info!("another updater is already running");

		return Ok(());
	};

	let engine = Arc::new(
		Engine::open(&data)
			.await
			.map_err(|error| error.to_string())?,
	);
	let mode = engine.update_settings().await.mode;

	if mode != UpdateMode::Daemon {
		tracing::info!(
			mode = mode.as_str(),
			"the update mode is not the background one, so there is nothing to do"
		);

		return Ok(());
	}

	tracing::info!(
		version = launcher_model::product::VERSION,
		"the background updater started"
	);

	let service = Service::new(
		Arc::new(backend::EngineBackend::new(Arc::clone(&engine))),
		Arc::new(SystemClock),
		std::process::id(),
		mode.as_str(),
	);

	#[cfg(unix)]
	let socket_task = {
		let listener = ipc::bind(&paths.socket)
			.await
			.map_err(|error| error.to_string())?;
		let socket = paths.socket.clone();
		let handler: Arc<dyn ipc::Handler> = Arc::new(Arc::clone(&service));
		let stop = service.stopped();

		tokio::spawn(async move { ipc::serve(listener, &socket, handler, stop).await })
	};

	let signals = {
		let service = Arc::clone(&service);

		tokio::spawn(async move {
			wait_for_signal().await;
			tracing::info!("asked to stop");
			service.request_stop();
		})
	};

	Arc::clone(&service).run().await;

	signals.abort();

	#[cfg(unix)]
	{
		service.request_stop();
		let _ = socket_task.await;
		let _ = std::fs::remove_file(&paths.socket);
	}

	if let Some(engine) = Arc::into_inner(engine) {
		engine.shutdown().await;
	}

	tracing::info!("the background updater stopped");

	Ok(())
}

#[cfg(unix)]
async fn wait_for_signal() {
	use tokio::signal::unix::{SignalKind, signal};

	let (Ok(mut terminate), Ok(mut interrupt)) = (
		signal(SignalKind::terminate()),
		signal(SignalKind::interrupt()),
	) else {
		std::future::pending::<()>().await;

		return;
	};

	tokio::select! {
		_ = terminate.recv() => {}
		_ = interrupt.recv() => {}
	}
}

#[cfg(not(unix))]
async fn wait_for_signal() {
	let _ = tokio::signal::ctrl_c().await;
}
