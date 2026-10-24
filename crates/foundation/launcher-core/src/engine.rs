use std::sync::Arc;

use launcher_model::{LauncherError, ThemePreference};
use launcher_net::HttpClient;
use launcher_storage::Database;
use tokio::sync::broadcast;

use crate::catalog::CatalogService;
use crate::config::EngineConfig;
use crate::credentials::{Keychain, SecretStore};
use crate::events::{EngineEvent, EventBus};
use crate::explore::{self, ExploreQuery, ExploreRow};
use crate::lifecycle::{HttpArtifacts, InstalledApp, LifecycleService, OperationSummary};
use crate::network;
use crate::paths::DataDirectory;
use crate::self_update::ReleaseConfig;

pub(crate) fn now_unix() -> i64 {
	time::OffsetDateTime::now_utc().unix_timestamp()
}

const THEME_KEY: &str = "theme";

/// The launcher's application service.
///
/// Share it behind an `Arc`. Every method is async and none block, so it is
/// safe to call from a task spawned by the interface.
pub struct Engine {
	pub(crate) data: DataDirectory,
	pub(crate) database: Arc<Database>,
	pub(crate) http: HttpClient,
	/// Keeps the GitHub token.
	pub(crate) secrets: Arc<dyn SecretStore>,
	/// Identifies this process, so "once per launch" can be told apart from
	/// "once per day".
	pub(crate) session_id: String,
	catalog: CatalogService,
	lifecycle: LifecycleService,
	pub(crate) events: EventBus,
	/// Where launcher releases come from, and which keys may sign them.
	pub(crate) releases: ReleaseConfig,
	/// One launcher download at a time.
	pub(crate) launcher_update: tokio::sync::Mutex<()>,
	/// The website's daily release snapshot, once it has been read.
	pub(crate) observations: crate::updates::ObservationsCache,
	/// Until this Unix time, update checks do not ask GitHub, because it just
	/// said its request limit was reached. A new token clears it.
	pub(crate) github_cooldown_until: std::sync::atomic::AtomicI64,
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
		let secrets: Arc<dyn SecretStore> =
			config.secrets.clone().unwrap_or_else(|| Arc::new(Keychain));
		let preferences = network::read_preferences(&database).await;

		launcher_model::local_time::configure(preferences.time_format, preferences.date_format);
		launcher_model::local_time::set_language(
			preferences
				.language
				.resolve(launcher_model::local_time::system_locale()),
		);

		// The token is read once here. Only a person's action in Settings
		// changes it afterwards.
		let token = if preferences.network.token_saved {
			let secrets = Arc::clone(&secrets);

			tokio::task::spawn_blocking(move || secrets.get().ok().flatten())
				.await
				.ok()
				.flatten()
		} else {
			None
		};
		let token = std::env::var(network::TOKEN_VARIABLE)
			.ok()
			.filter(|value| !value.trim().is_empty())
			.or(token);
		let http = HttpClient::with_options(&network::network_options(&preferences, token))?;
		let catalog = CatalogService::new(config.catalog, Arc::clone(&database), http.clone());

		catalog.load_cached().await;

		let events = EventBus::new();
		let artifacts = config
			.artifacts
			.unwrap_or_else(|| Arc::new(HttpArtifacts(http.clone())));
		let lifecycle = LifecycleService::new(
			data.clone(),
			Arc::clone(&database),
			artifacts,
			events.clone(),
		);

		// A crash can leave half-finished work. Settle it before anything new
		// starts, so the first screen reflects the real state.
		match lifecycle.recover(now_unix()).await {
			Ok(report) => tracing::info!(?report, "startup recovery finished"),
			Err(error) => tracing::warn!(%error, "startup recovery could not run"),
		}

		Ok(Self {
			data: data.clone(),
			database,
			http,
			secrets,
			session_id: launcher_model::OperationId::new().to_string(),
			catalog,
			lifecycle,
			events,
			releases: config.releases,
			launcher_update: tokio::sync::Mutex::new(()),
			observations: crate::updates::ObservationsCache::default(),
			github_cooldown_until: std::sync::atomic::AtomicI64::new(0),
		})
	}

	/// Everything installed, by name.
	pub async fn installed(&self) -> Result<Vec<InstalledApp>, LauncherError> {
		Ok(self
			.database
			.installations()
			.await?
			.into_iter()
			.map(InstalledApp::from)
			.collect())
	}

	/// Where the launcher keeps its data.
	pub fn data_directory(&self) -> &DataDirectory {
		&self.data
	}

	/// Launcher bookkeeping the background updater keeps between runs, such as
	/// when it last scanned. These are plain values, never settings that grant
	/// anything.
	pub async fn state_value(&self, key: &'static str) -> Option<String> {
		self.database.setting(key).await.ok().flatten()
	}

	pub async fn set_state_value(
		&self,
		key: &'static str,
		value: String,
	) -> Result<(), LauncherError> {
		self.database.set_setting(key, value).await
	}

	/// The most recent installs, updates, and removals, newest first.
	pub async fn history(&self, limit: u32) -> Result<Vec<OperationSummary>, LauncherError> {
		Ok(self
			.database
			.recent_operations(limit)
			.await?
			.into_iter()
			.map(OperationSummary::from)
			.collect())
	}

	pub fn lifecycle(&self) -> &LifecycleService {
		&self.lifecycle
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
		drop(self.lifecycle);

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
