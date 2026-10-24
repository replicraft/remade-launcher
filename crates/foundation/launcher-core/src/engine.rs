use launcher_model::{LauncherError, ThemePreference};
use launcher_net::HttpClient;
use launcher_storage::Database;
use tokio::sync::broadcast;

use crate::events::{EngineEvent, EventBus};
use crate::paths::DataDirectory;

const THEME_KEY: &str = "theme";

/// The launcher's application service.
///
/// Share it behind an `Arc`. Every method is async and none block, so it is
/// safe to call from a task spawned by the interface.
pub struct Engine {
	database: Database,
	http: HttpClient,
	events: EventBus,
}

impl Engine {
	/// Opens local storage and builds the shared HTTP client.
	pub async fn open(data: &DataDirectory) -> Result<Self, LauncherError> {
		data.ensure_exists()?;

		let database = Database::open(data.database_file()).await?;
		let http = HttpClient::new()?;

		Ok(Self {
			database,
			http,
			events: EventBus::new(),
		})
	}

	/// The shared HTTP client, for the services that will fetch catalogs and
	/// release metadata.
	pub fn http(&self) -> &HttpClient {
		&self.http
	}

	pub fn subscribe(&self) -> broadcast::Receiver<EngineEvent> {
		self.events.subscribe()
	}

	/// The saved theme. An unreadable or unknown value falls back to following
	/// the system rather than blocking startup.
	pub async fn theme(&self) -> ThemePreference {
		match self.database.setting(THEME_KEY).await {
			Ok(Some(stored)) => stored.parse().unwrap_or_default(),
			Ok(None) => ThemePreference::default(),
			Err(error) => {
				tracing::warn!(%error, "could not read the saved theme");
				ThemePreference::default()
			}
		}
	}

	pub async fn set_theme(&self, theme: ThemePreference) -> Result<(), LauncherError> {
		self.database
			.set_setting(THEME_KEY, theme.as_str().to_owned())
			.await?;
		self.events.publish(EngineEvent::ThemeChanged(theme));

		Ok(())
	}

	/// Flushes and closes storage. Call once, when the app is quitting.
	pub async fn shutdown(self) {
		self.database.close().await;
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	async fn open_temporary() -> (tempfile::TempDir, Engine) {
		let directory = tempfile::tempdir().unwrap();
		let engine = Engine::open(&DataDirectory::at(directory.path().join("data")))
			.await
			.unwrap();

		(directory, engine)
	}

	#[tokio::test]
	async fn theme_defaults_to_system() {
		let (_directory, engine) = open_temporary().await;

		assert_eq!(engine.theme().await, ThemePreference::System);
	}

	#[tokio::test]
	async fn theme_is_saved_and_announced() {
		let (_directory, engine) = open_temporary().await;
		let mut events = engine.subscribe();

		engine.set_theme(ThemePreference::Dark).await.unwrap();

		assert_eq!(engine.theme().await, ThemePreference::Dark);
		assert_eq!(
			events.recv().await.unwrap(),
			EngineEvent::ThemeChanged(ThemePreference::Dark)
		);
	}

	#[tokio::test]
	async fn an_unknown_stored_theme_falls_back() {
		let (_directory, engine) = open_temporary().await;
		engine
			.database
			.set_setting(THEME_KEY, "sepia".into())
			.await
			.unwrap();

		assert_eq!(engine.theme().await, ThemePreference::System);
	}
}
