use std::process::ExitCode;
use std::sync::Arc;

use launcher_core::{Engine, OperationSummary};
use launcher_model::LauncherError;
use serde::Serialize;

use crate::cli::{Cli, HistoryArgs};
use crate::output::{date, print_json, print_table, truncate};

#[derive(Serialize)]
struct Entry<'a> {
	date: String,
	operation: &'a str,
	result: &'a str,
	source: Option<&'a str>,
	detail: Option<&'a str>,
}

pub(crate) async fn run(
	engine: &Arc<Engine>,
	cli: &Cli,
	args: &HistoryArgs,
) -> Result<ExitCode, LauncherError> {
	let history = engine.history(args.limit).await?;

	if cli.json {
		let entries: Vec<Entry<'_>> = history.iter().map(entry).collect();
		print_json(&entries);

		return Ok(ExitCode::SUCCESS);
	}

	if history.is_empty() {
		eprintln!("Nothing has been installed or removed yet.");

		return Ok(ExitCode::SUCCESS);
	}

	let rows: Vec<Vec<String>> = history
		.iter()
		.map(|operation| {
			vec![
				date(operation.started_at),
				operation.kind.clone(),
				operation.state.clone(),
				operation.app.clone().unwrap_or_default(),
				truncate(operation.detail.as_deref().unwrap_or_default(), 50),
			]
		})
		.collect();

	print_table(&["DATE", "OPERATION", "RESULT", "SOURCE", "DETAIL"], &rows);

	Ok(ExitCode::SUCCESS)
}

fn entry(operation: &OperationSummary) -> Entry<'_> {
	Entry {
		date: date(operation.started_at),
		operation: &operation.kind,
		result: &operation.state,
		source: operation.app.as_deref(),
		detail: operation.detail.as_deref(),
	}
}
