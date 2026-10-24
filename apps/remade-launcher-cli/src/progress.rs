//! Progress from engine events: a bar for people, NDJSON for programs.

use std::io::IsTerminal;
use std::sync::Arc;

use indicatif::{ProgressBar, ProgressStyle};
use launcher_core::{Engine, EngineEvent, Stage};
use serde_json::json;
use tokio::task::JoinHandle;

use crate::context::{Ctx, Format};

pub(crate) struct Printer {
	task: JoinHandle<()>,
}

/// The phase name used in NDJSON events.
fn phase(stage: &Stage) -> &'static str {
	match stage {
		Stage::Accepted => "accepted",
		Stage::Downloading { .. } => "downloading",
		Stage::Verifying => "verifying",
		Stage::Inspecting => "inspecting",
		Stage::Extracting => "extracting",
		Stage::Applying => "applying",
		Stage::Removing => "removing",
		Stage::Completed => "completed",
		Stage::Failed { .. } => "failed",
		Stage::RecoveryRequired { .. } => "recovery_required",
	}
}

impl Printer {
	/// Starts reporting events for operations on this engine.
	pub(crate) fn start(engine: &Arc<Engine>, ctx: &Arc<Ctx>) -> Self {
		let mut events = engine.subscribe();
		let ctx = Arc::clone(ctx);
		let draw_bar = ctx.format == Format::Text
			&& !ctx.quiet
			&& !ctx.no_progress
			&& std::io::stderr().is_terminal();
		let ndjson = ctx.format == Format::Ndjson;

		let task = tokio::spawn(async move {
			let bar = ProgressBar::hidden();

			if draw_bar {
				bar.set_draw_target(indicatif::ProgressDrawTarget::stderr());
				bar.set_style(
					ProgressStyle::with_template("{msg:<24} [{bar:30}] {bytes}/{total_bytes}")
						.unwrap_or_else(|_| ProgressStyle::default_bar())
						.progress_chars("=> "),
				);
			}

			let mut last_phase = "";
			let mut last_percent = u64::MAX;

			while let Ok(event) = events.recv().await {
				let EngineEvent::Operation(event) = event else {
					continue;
				};

				let name = phase(&event.stage);

				if let Stage::Downloading { received, total } = &event.stage {
					bar.set_length(total.unwrap_or(0));
					bar.set_position(*received);

					// A line per 5%, or per 4 MiB when the size is unknown: a
					// stream a program can follow without being flooded.
					let percent =
						total.map_or(*received >> 22, |total| received * 20 / total.max(1));

					if ndjson && percent != last_percent {
						last_percent = percent;
						ctx.event(json!({
							"operation_id": event.operation.to_string(),
							"installation_id": event.installation.to_string(),
							"phase": name,
							"current": received,
							"total": total,
						}));
					}
				} else if ndjson && name != last_phase {
					let message = match &event.stage {
						Stage::Failed { message } | Stage::RecoveryRequired { message } => {
							Some(message.clone())
						}
						_ => None,
					};

					ctx.event(json!({
						"operation_id": event.operation.to_string(),
						"installation_id": event.installation.to_string(),
						"phase": name,
						"message": message,
					}));
				}

				if event.stage.is_terminal() {
					bar.finish_and_clear();

					break;
				}

				bar.set_message(name.to_owned());

				// With no terminal, a person reading a log gets each phase once.
				if ctx.format == Format::Text
					&& !draw_bar && !ctx.quiet
					&& !ctx.no_progress
					&& name != last_phase
				{
					eprintln!("{name}…");
				}

				last_phase = name;
			}
		});

		Self { task }
	}

	pub(crate) async fn finish(self) {
		// The printer ends on a terminal event. If the operation failed before
		// emitting one, stop waiting.
		self.task.abort();
		let _ = self.task.await;
	}
}

/// The bar and events for one plain download, such as `remade download` or a
/// new launcher: a bar on a terminal, a line per few percent for programs.
pub(crate) struct DownloadProgress {
	bar: ProgressBar,
	ctx: Arc<Ctx>,
}

impl DownloadProgress {
	pub(crate) fn start(ctx: &Arc<Ctx>) -> Self {
		let bar = ProgressBar::hidden();

		if ctx.format == Format::Text
			&& !ctx.quiet
			&& !ctx.no_progress
			&& std::io::stderr().is_terminal()
		{
			bar.set_draw_target(indicatif::ProgressDrawTarget::stderr());
			bar.set_style(
				ProgressStyle::with_template("Downloading [{bar:30}] {bytes}/{total_bytes}")
					.unwrap_or_else(|_| ProgressStyle::default_bar())
					.progress_chars("=> "),
			);
		}

		Self {
			bar,
			ctx: Arc::clone(ctx),
		}
	}

	/// What the engine calls as bytes arrive.
	pub(crate) fn callback(&self) -> impl FnMut(u64, Option<u64>) + Send + use<> {
		let bar = self.bar.clone();
		let ctx = Arc::clone(&self.ctx);
		let ndjson = ctx.format == Format::Ndjson;
		let mut last_percent = u64::MAX;

		move |received, total| {
			bar.set_length(total.unwrap_or(0));
			bar.set_position(received);

			let percent = total.map_or(received >> 22, |total| received * 20 / total.max(1));

			if ndjson && percent != last_percent {
				last_percent = percent;
				ctx.event(json!({ "phase": "downloading", "received": received, "total": total }));
			}
		}
	}

	pub(crate) fn finish(&self) {
		self.bar.finish_and_clear();
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn every_stage_has_a_distinct_phase_name() {
		let stages = [
			Stage::Accepted,
			Stage::Downloading {
				received: 0,
				total: None,
			},
			Stage::Verifying,
			Stage::Inspecting,
			Stage::Extracting,
			Stage::Applying,
			Stage::Removing,
			Stage::Completed,
			Stage::Failed {
				message: String::new(),
			},
			Stage::RecoveryRequired {
				message: String::new(),
			},
		];
		let names: std::collections::HashSet<_> = stages.iter().map(phase).collect();

		assert_eq!(names.len(), stages.len());
	}
}
