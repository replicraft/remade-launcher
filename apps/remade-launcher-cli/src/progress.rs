//! Showing install progress from engine events.

use std::io::IsTerminal;
use std::sync::Arc;

use indicatif::{ProgressBar, ProgressStyle};
use launcher_core::{Engine, EngineEvent, Stage};
use tokio::task::JoinHandle;

pub(crate) struct Printer {
	task: JoinHandle<()>,
}

impl Printer {
	/// Starts printing events for operations on this engine. Progress goes to
	/// stderr, so stdout stays clean for results.
	pub(crate) fn start(engine: &Arc<Engine>) -> Self {
		let mut events = engine.subscribe();
		let interactive = std::io::stderr().is_terminal();

		let task = tokio::spawn(async move {
			let bar = ProgressBar::hidden();

			if interactive {
				bar.set_draw_target(indicatif::ProgressDrawTarget::stderr());
				bar.set_style(
					ProgressStyle::with_template("{msg:<24} [{bar:30}] {bytes}/{total_bytes}")
						.unwrap_or_else(|_| ProgressStyle::default_bar())
						.progress_chars("=> "),
				);
			}

			let mut last_label = "";

			while let Ok(event) = events.recv().await {
				let EngineEvent::Operation(event) = event else {
					continue;
				};

				let label = match &event.stage {
					Stage::Accepted => "Starting",
					Stage::Downloading { received, total } => {
						if let Some(total) = total {
							bar.set_length(*total);
						}

						bar.set_position(*received);
						"Downloading"
					}
					Stage::Verifying => "Checking the download",
					Stage::Inspecting => "Inspecting the package",
					Stage::Extracting => "Unpacking",
					Stage::Applying => "Finishing",
					Stage::Removing => "Removing",
					Stage::Completed | Stage::Failed { .. } | Stage::RecoveryRequired { .. } => {
						bar.finish_and_clear();

						break;
					}
				};

				bar.set_message(label);

				// Without a terminal, print each stage once instead of redrawing.
				if !interactive && label != last_label {
					eprintln!("{label}…");
				}

				last_label = label;
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
