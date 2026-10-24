//! A support report a person can read and paste.
//!
//! It states facts about the launcher and its state and leaves out anything
//! private: no folder paths, URLs, file names, or installed app names.

use std::fmt::Write;

use launcher_model::{LauncherError, product};

use crate::engine::Engine;

impl Engine {
	pub async fn diagnostics(&self) -> Result<String, LauncherError> {
		let mut report = String::new();
		let now = time::OffsetDateTime::now_utc();

		let _ = writeln!(report, "{} {}", product::NAME, product::VERSION);
		let _ = writeln!(
			report,
			"System: {} {}",
			std::env::consts::OS,
			std::env::consts::ARCH
		);

		let state = self.catalog().state();

		match &state.catalog {
			Some(catalog) => {
				let _ = writeln!(
					report,
					"Catalog: revision {}, {} apps, {}",
					catalog.revision(),
					catalog.project_count(),
					match catalog.freshness(now) {
						launcher_catalog::Freshness::Current => "current",
						launcher_catalog::Freshness::Expired => "expired",
					}
				);
			}
			None => {
				let _ = writeln!(report, "Catalog: none verified");
			}
		}

		if let Some(problem) = &state.problem {
			let _ = writeln!(report, "Catalog problem: {:?}", problem.kind);
		}

		if state.clock_warning {
			let _ = writeln!(report, "Clock: earlier than a time already trusted");
		}

		let _ = writeln!(report, "Installed apps: {}", self.installed().await?.len());

		let history = self.history(10).await?;
		let _ = writeln!(report, "Recent operations (newest first):");

		for operation in &history {
			let _ = writeln!(report, "  {} {}", operation.kind, operation.state);
		}

		if history.is_empty() {
			let _ = writeln!(report, "  none");
		}

		Ok(report)
	}
}

#[cfg(test)]
mod tests {
	use crate::{DataDirectory, Engine};

	#[tokio::test]
	async fn the_report_has_the_facts_and_none_of_the_private_details() {
		let directory = tempfile::tempdir().unwrap();
		let data = DataDirectory::at(directory.path().join("secret-folder-name"));
		let engine = Engine::open(&data).await.unwrap();

		let report = engine.diagnostics().await.unwrap();

		assert!(report.contains("Remade Launcher"));
		assert!(report.contains("Catalog: none verified"));
		assert!(report.contains("Installed apps: 0"));
		assert!(!report.contains("secret-folder-name"));
		assert!(!report.contains(directory.path().to_str().unwrap()));
	}
}
