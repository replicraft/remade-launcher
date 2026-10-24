//! `remade`: the Remade command line.
//!
//! It drives the same engine as the desktop app, so a catalog entry means
//! the same thing in both, and the app and the CLI can be used side by side.

mod app_ref;
mod cli;
mod commands;
mod output;
mod progress;
mod prompt;

use std::process::ExitCode;
use std::sync::Arc;

use clap::Parser;
use launcher_core::{DataDirectory, Engine};
use tracing_subscriber::EnvFilter;

#[global_allocator]
static ALLOCATOR: mimalloc::MiMalloc = mimalloc::MiMalloc;

fn main() -> ExitCode {
	let arguments = cli::Cli::parse();

	// Logs go to stderr, and stay quiet unless asked for.
	tracing_subscriber::fmt()
		.with_writer(std::io::stderr)
		.with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "error".into()))
		.init();

	let runtime = match tokio::runtime::Builder::new_multi_thread()
		.worker_threads(4)
		.thread_name("remade-async")
		.enable_all()
		.build()
	{
		Ok(runtime) => runtime,
		Err(error) => {
			eprintln!("error: could not start the async runtime: {error}");

			return ExitCode::FAILURE;
		}
	};

	let code = runtime.block_on(run(arguments));

	// Stragglers such as a logger task must not hold the process open.
	runtime.shutdown_background();

	code
}

async fn run(arguments: cli::Cli) -> ExitCode {
	let data = match &arguments.data_dir {
		Some(path) => DataDirectory::at(path),
		None => match DataDirectory::platform_default() {
			Ok(data) => data,
			Err(error) => return output::failure(&error),
		},
	};

	let engine = match Engine::open(&data).await {
		Ok(engine) => Arc::new(engine),
		Err(error) => return output::failure(&error),
	};

	let result = commands::dispatch(&engine, &arguments).await;

	if let Some(engine) = Arc::into_inner(engine) {
		engine.shutdown().await;
	}

	match result {
		Ok(code) => code,
		Err(error) => output::failure(&error),
	}
}
