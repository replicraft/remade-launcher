//! `remade`: the Remade command line.
//!
//! It drives the same engine as the desktop app, so a catalog entry means
//! the same thing in both, and the two can be used side by side. Output
//! format is only formatting: `--json` never changes what a command may do
//! or what it asks first.

mod app_ref;
mod auto_update;
mod background;
mod cli;
mod commands;
mod context;
mod errors;
mod exit;
mod output;
mod progress;
mod prompt;

use std::process::ExitCode;
use std::sync::Arc;

use clap::Parser;
use cli::{Cli, OutputFormat};
use context::{Ctx, Format};
use errors::CliError;
use exit::ExitStatus;
use launcher_core::{DataDirectory, Engine};
use serde_json::json;
use tracing_subscriber::EnvFilter;

#[global_allocator]
static ALLOCATOR: mimalloc::MiMalloc = mimalloc::MiMalloc;

fn main() -> ExitCode {
	let arguments = match Cli::try_parse() {
		Ok(arguments) => arguments,
		Err(error) => return usage_error(&error),
	};

	// Logs go to stderr, and stay quiet unless asked for.
	let default_level = if arguments.verbose { "debug" } else { "error" };

	tracing_subscriber::fmt()
		.with_writer(std::io::stderr)
		.with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| default_level.into()))
		.init();

	let format = match (arguments.json, arguments.output) {
		(true, _) | (_, OutputFormat::Json) => Format::Json,
		(_, OutputFormat::Ndjson) => Format::Ndjson,
		(_, OutputFormat::Text) => Format::Text,
	};

	let ctx = Arc::new(Ctx::new(
		format,
		arguments.quiet,
		arguments.no_progress,
		arguments.no_input,
		arguments.offline,
		commands::name(&arguments.command),
	));

	let runtime = match tokio::runtime::Builder::new_multi_thread()
		.worker_threads(4)
		.thread_name("remade-async")
		.enable_all()
		.build()
	{
		Ok(runtime) => runtime,
		Err(error) => {
			let error = CliError::new(
				"INTERNAL",
				ExitStatus::Internal,
				format!("could not start the async runtime: {error}"),
			);

			return output::report(&ctx, &error);
		}
	};

	let code = runtime.block_on(run(&ctx, &arguments));

	// Stragglers such as a logger task must not hold the process open.
	runtime.shutdown_background();

	code
}

async fn run(ctx: &Arc<Ctx>, arguments: &Cli) -> ExitCode {
	let work = execute(ctx, arguments);

	let result = match arguments.timeout {
		Some(seconds) => {
			match tokio::time::timeout(std::time::Duration::from_secs(seconds), work).await {
				Ok(result) => result,
				Err(_) => Err(CliError::new(
					"TIMED_OUT",
					ExitStatus::Network,
					format!("The command took longer than {seconds} seconds."),
				)),
			}
		}
		None => work.await,
	};

	match result {
		Ok(status) => status.into(),
		Err(error) => output::report(ctx, &error),
	}
}

async fn execute(ctx: &Arc<Ctx>, arguments: &Cli) -> Result<ExitStatus, CliError> {
	// Completions need no engine, so they work before anything is set up.
	if let cli::Command::Completions { shell } = &arguments.command {
		return Ok(commands::completions_only(*shell));
	}

	let data = match &arguments.data_dir {
		Some(path) => DataDirectory::at(path),
		None => DataDirectory::platform_default()?,
	};

	let engine = Arc::new(Engine::open(&data).await?);
	let result = commands::dispatch(ctx, &engine, arguments).await;

	// A person at a terminal also gets the launcher's own updates, by the
	// setting. It runs after the command, so it never delays or changes it.
	if matches!(result, Ok(ExitStatus::Success)) {
		auto_update::run(ctx, &engine).await;
	}

	if let Some(engine) = Arc::into_inner(engine) {
		engine.shutdown().await;
	}

	result
}

/// A command line clap could not use. Help and version print as usual. A real
/// mistake exits with the usage status, and when the caller asked for JSON it
/// is reported as the same typed error object every other failure uses.
fn usage_error(error: &clap::Error) -> ExitCode {
	if !error.use_stderr() {
		let _ = error.print();

		return ExitCode::SUCCESS;
	}

	let wants_json = std::env::args().any(|argument| {
		matches!(
			argument.as_str(),
			"--json" | "--output=json" | "--output=ndjson"
		)
	}) || std::env::args()
		.zip(std::env::args().skip(1))
		.any(|(flag, value)| flag == "--output" && value != "text");

	if wants_json {
		let message = error.render().to_string();
		let first = message
			.lines()
			.next()
			.unwrap_or("invalid arguments")
			.trim_start_matches("error: ")
			.to_owned();

		println!(
			"{}",
			json!({
				"schema_version": context::SCHEMA_VERSION,
				"command": null,
				"ok": false,
				"error": { "code": "USAGE", "message": first, "retryable": false, "exit_status": ExitStatus::Usage.code() },
				"warnings": [],
			})
		);
		eprintln!("{first}");

		return ExitStatus::Usage.into();
	}

	let _ = error.print();

	ExitStatus::Usage.into()
}
