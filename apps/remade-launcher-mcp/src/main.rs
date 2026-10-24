//! Remade Launcher for AI assistants, over the Model Context Protocol.
//!
//! This is the same engine the app and the command line use, offered as
//! tools. Assistants get three tiers, and each higher one has to be turned on
//! by the person who starts the server:
//!
//! 1. Reading: search, inspect, list what is installed, check for updates.
//! 2. Planning: work out exactly what an install, update, repair, or removal
//!    would do, and save it as a plan. Nothing changes.
//! 3. Applying a saved plan, which only works when the server was started with
//!    `--allow-apply` (and `--allow-destructive` for removals).
//!
//! Applying names the plan, the app, and the version, so an assistant can only
//! approve exactly what it showed. Apps from sources Remade has not reviewed
//! cannot be applied this way at all: that needs a person at the command line.

mod server;

use std::sync::Arc;

use launcher_core::{DataDirectory, Engine};
use rmcp::ServiceExt;
use tracing_subscriber::EnvFilter;

#[global_allocator]
static ALLOCATOR: mimalloc::MiMalloc = mimalloc::MiMalloc;

/// What the person who started the server allows.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Permissions {
	pub(crate) apply: bool,
	pub(crate) destructive: bool,
}

fn parse(arguments: &[String]) -> Result<Permissions, String> {
	let mut permissions = Permissions::default();

	for argument in arguments {
		match argument.as_str() {
			"--allow-apply" => permissions.apply = true,
			"--allow-destructive" => permissions.destructive = true,
			"--help" | "-h" => {
				return Err(
					"remade-launcher-mcp [--allow-apply] [--allow-destructive]\n\nBy default assistants can look things up and plan changes, but not make them.\n--allow-apply lets them apply a saved plan; --allow-destructive also lets them apply removals."
						.to_owned(),
				);
			}
			other => return Err(format!("Unknown option {other}. Try --help.")),
		}
	}

	if permissions.destructive && !permissions.apply {
		return Err("--allow-destructive only makes sense with --allow-apply.".to_owned());
	}

	Ok(permissions)
}

#[tokio::main(flavor = "multi_thread", worker_threads = 4)]
async fn main() -> std::process::ExitCode {
	// Standard output carries the protocol, so logs go to standard error.
	tracing_subscriber::fmt()
		.with_writer(std::io::stderr)
		.with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "error".into()))
		.init();

	let arguments: Vec<String> = std::env::args().skip(1).collect();
	let permissions = match parse(&arguments) {
		Ok(permissions) => permissions,
		Err(message) => {
			eprintln!("{message}");

			return std::process::ExitCode::from(2);
		}
	};

	let data = match std::env::var_os("REMADE_DATA_DIR") {
		Some(path) if !path.is_empty() => DataDirectory::at(path),
		_ => match DataDirectory::platform_default() {
			Ok(data) => data,
			Err(error) => {
				eprintln!("{error}");

				return std::process::ExitCode::from(10);
			}
		},
	};

	let engine = match Engine::open(&data).await {
		Ok(engine) => Arc::new(engine),
		Err(error) => {
			eprintln!("{error}");

			return std::process::ExitCode::from(10);
		}
	};

	let service = match server::Remade::new(Arc::clone(&engine), permissions)
		.serve(rmcp::transport::stdio())
		.await
	{
		Ok(service) => service,
		Err(error) => {
			eprintln!("could not start: {error}");

			return std::process::ExitCode::from(10);
		}
	};

	let _ = service.waiting().await;

	std::process::ExitCode::SUCCESS
}

#[cfg(test)]
mod tests {
	use super::*;

	fn words(text: &str) -> Vec<String> {
		text.split_whitespace().map(str::to_owned).collect()
	}

	#[test]
	fn nothing_is_allowed_unless_asked_for() {
		let none = parse(&[]).unwrap();

		assert!(!none.apply && !none.destructive);
		assert!(parse(&words("--allow-apply")).unwrap().apply);
		assert!(
			parse(&words("--allow-apply --allow-destructive"))
				.unwrap()
				.destructive
		);
	}

	#[test]
	fn removals_cannot_be_allowed_without_applying_and_unknown_flags_are_refused() {
		assert!(parse(&words("--allow-destructive")).is_err());
		assert!(parse(&words("--allow-everything")).is_err());
		assert!(parse(&words("--insecure")).is_err());
	}
}
