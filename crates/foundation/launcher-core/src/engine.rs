use std::sync::Arc;

use launcher_model::{LauncherError, ThemePreference};
use launcher_net::HttpClient;
use launcher_storage::Database;
use tokio::sync::broadcast;

use crate::catalog::CatalogService;
use crate::config::EngineConfig;
use crate::events::{EngineEvent, EventBus};
use crate::explore::{self, ExploreQuery, ExploreRow};
use crate::paths::DataDirectory;

const THEME_KEY: &str = "theme";

/// The launcher's application service.
///
/// Share it behind an `Arc`. Every method is async and none block, so it is
/// safe to call from a task spawned by the interface.
pub struct Engine {
	database: Arc<Database>,
	http: HttpClient,
	catalog: CatalogService,
	events: EventBus,
}

impl Engine {
	/// Opens the engine with the official catalog configuration.
	pub async fn open(data: &DataDirectory) -> Result<Self, LauncherError> {
		Self::open_with(data, EngineConfig::production()).await
	}

	/// Opens local storage, builds the shared HTTP client, and loads the
	/// stored catalog (after verifying it again).
	pub async fn open_with(
		data: &DataDirectory,
		config: EngineConfig,
	) -> Result<Self, LauncherError> {
		data.ensure_exists()?;

		let database = Arc::new(Database::open(data.database_file()).await?);
		let http = HttpClient::new()?;
		let catalog = CatalogService::new(config.catalog, Arc::clone(&database), http.clone());

		catalog.load_cached().await;

		Ok(Self {
			database,
			http,
			catalog,
			events: EventBus::new(),
		})
	}

	pub fn catalog(&self) -> &CatalogService {
		&self.catalog
	}

	/// Searches the verified catalog. Returns nothing while no catalog is
	/// available.
	pub async fn explore(&self, query: ExploreQuery) -> Vec<ExploreRow> {
		match self.catalog.state().catalog.clone() {
			Some(catalog) => {
				explore::search(catalog, query, self.catalog.config().supported_capabilities).await
			}
			None => Vec::new(),
		}
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
		drop(self.catalog);

		if let Some(database) = Arc::into_inner(self.database) {
			database.close().await;
		}
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
