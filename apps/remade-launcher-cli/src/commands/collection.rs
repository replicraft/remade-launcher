//! Collections: a portable list of apps to consider.
//!
//! Importing only adds favorites. It never installs anything, and a
//! repository in a collection never counts as reviewed.

use std::io::Read;
use std::path::Path;
use std::sync::Arc;

use launcher_core::{Collection, CollectionEntry, Engine, EntryOutcome};
use serde_json::{Value, json};

use crate::cli::CollectionAction;
use crate::context::Ctx;
use crate::errors::CliError;
use crate::exit::ExitStatus;
use crate::output::refuse;
use crate::prompt;

/// Reads a collection file without trusting its size.
fn read_file(path: &str) -> Result<Collection, CliError> {
	let limit = u64::try_from(launcher_core::collection_max_bytes()).unwrap_or(u64::MAX);
	let file = std::fs::File::open(Path::new(path))
		.map_err(|_| CliError::usage(format!("{path} could not be opened.")))?;
	let mut bytes = Vec::new();

	file.take(limit + 1)
		.read_to_end(&mut bytes)
		.map_err(|_| CliError::usage(format!("{path} could not be read.")))?;

	Collection::parse(&bytes).map_err(|problem| CliError::usage(problem.to_string()))
}

fn entry_json(entry: &CollectionEntry) -> Value {
	json!({ "catalog_name": entry.catalog_name, "repository": entry.repository, "note": entry.note })
}

fn label(entry: &CollectionEntry) -> String {
	entry
		.catalog_name
		.clone()
		.or_else(|| entry.repository.clone())
		.unwrap_or_default()
}

fn outcome_text(outcome: &EntryOutcome) -> String {
	match outcome {
		EntryOutcome::InCatalog { name } => format!("in the catalog as {name}"),
		EntryOutcome::NotInCatalog => "not in the catalog (it may be newer than yours)".to_owned(),
		EntryOutcome::Repository => "a GitHub repository (not reviewed by Remade)".to_owned(),
		EntryOutcome::AlreadyInstalled => "already installed".to_owned(),
	}
}

fn outcome_code(outcome: &EntryOutcome) -> &'static str {
	match outcome {
		EntryOutcome::InCatalog { .. } => "in_catalog",
		EntryOutcome::NotInCatalog => "not_in_catalog",
		EntryOutcome::Repository => "repository_unreviewed",
		EntryOutcome::AlreadyInstalled => "already_installed",
	}
}

pub(crate) async fn run(
	ctx: &Arc<Ctx>,
	engine: &Arc<Engine>,
	action: &CollectionAction,
) -> Result<ExitStatus, CliError> {
	match action {
		CollectionAction::Export { out } => {
			let collection = engine.export_collection().await?;

			match out {
				Some(path) => {
					let mut file = std::fs::OpenOptions::new()
						.write(true)
						.create_new(true)
						.open(path)
						.map_err(|_| {
							CliError::usage(format!(
								"{path} already exists or cannot be created, so nothing was written."
							))
						})?;

					std::io::Write::write_all(&mut file, collection.to_json().as_bytes())
						.map_err(|_| CliError::usage(format!("{path} could not be written.")))?;

					if ctx.machine() {
						ctx.emit(json!({ "saved_to": path, "apps": collection.apps.len() }));
					} else {
						println!("Saved {} apps to {path}.", collection.apps.len());
					}
				}
				None if ctx.machine() => ctx.emit(
					json!({ "collection": serde_json::to_value(&collection).unwrap_or_default() }),
				),
				None => println!("{}", collection.to_json()),
			}

			Ok(ExitStatus::Success)
		}
		CollectionAction::Preview { file } => {
			let collection = read_file(file)?;
			let preview = engine.preview_collection(&collection).await?;

			if ctx.machine() {
				ctx.emit(json!({
					"apps": preview.iter().map(|item| json!({ "entry": entry_json(&item.entry), "outcome": outcome_code(&item.outcome) })).collect::<Vec<_>>(),
					"effect": "favorites_only",
				}));
			} else {
				for item in &preview {
					println!("{:<32} {}", label(&item.entry), outcome_text(&item.outcome));
				}

				println!("Importing adds these as favorites. It installs nothing.");
			}

			Ok(ExitStatus::Success)
		}
		CollectionAction::Import { file, yes } => {
			let collection = read_file(file)?;
			let preview = engine.preview_collection(&collection).await?;

			if !ctx.machine() {
				for item in &preview {
					println!("{:<32} {}", label(&item.entry), outcome_text(&item.outcome));
				}
			}

			if !prompt::confirm(
				ctx,
				&format!("Add {} apps as favorites?", collection.apps.len()),
				*yes,
			)? {
				return Ok(refuse(
					ctx,
					"APPROVAL_REQUIRED",
					ExitStatus::ApprovalRequired,
					"Cancelled. Nothing was changed.",
				));
			}

			let added = engine.import_collection(&collection).await?;

			if ctx.machine() {
				ctx.emit(
					json!({ "added": added, "already_there": collection.apps.len() - added, "installed": 0 }),
				);
			} else {
				println!("Added {added} favorites. Nothing was installed.");
			}

			Ok(ExitStatus::Success)
		}
		CollectionAction::List => {
			let favorites = engine.favorites().await?;

			if ctx.machine() {
				ctx.emit(
					json!({ "favorites": favorites.iter().map(entry_json).collect::<Vec<_>>() }),
				);
			} else if favorites.is_empty() {
				ctx.note("No favorites yet. Import a collection to add some.");
			} else {
				for entry in &favorites {
					println!("{}", label(entry));
				}
			}

			Ok(ExitStatus::Success)
		}
		CollectionAction::Remove { reference } => {
			let removed = engine.remove_favorite(reference).await?;

			if !removed {
				return Ok(refuse(
					ctx,
					"NOT_FOUND",
					ExitStatus::Usage,
					&format!("{reference} is not a favorite."),
				));
			}

			if ctx.machine() {
				ctx.emit(json!({ "removed": reference }));
			} else {
				println!("Removed {reference} from your favorites.");
			}

			Ok(ExitStatus::Success)
		}
	}
}
