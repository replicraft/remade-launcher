//! Preferences, the GitHub token, and how the launcher reaches the network.

use launcher_model::{ErrorCode, LauncherError, Preferences, local_time};
use launcher_net::{HttpClient, MetadataRequest, NetworkOptions, Url};
use launcher_providers::{CachedTransport, GithubProvider, ResponseStore, StoredResponse};
use launcher_storage::Database;
use serde::Deserialize;

use crate::credentials::{TokenState, clean_token, token_looks_valid};
use crate::engine::{Engine, now_unix};

pub(crate) const PREFERENCES_KEY: &str = "preferences";
/// A token supplied this way beats the one in the credential store, which
/// suits scripts and CI.
pub const TOKEN_VARIABLE: &str = "REMADE_GITHUB_TOKEN";

/// Reads saved preferences. Anything unreadable falls back to the defaults
/// instead of stopping the launcher from starting.
pub(crate) async fn read_preferences(database: &Database) -> Preferences {
	match database.setting(PREFERENCES_KEY).await {
		Ok(Some(text)) => match serde_json::from_str::<Preferences>(&text) {
			Ok(preferences) => preferences.sanitized(),
			Err(error) => {
				tracing::warn!(%error, "saved preferences were unreadable, using the defaults");
				Preferences::default()
			}
		},
		Ok(None) => Preferences::default(),
		Err(error) => {
			tracing::warn!(%error, "could not read the saved preferences");
			Preferences::default()
		}
	}
}

/// What the client needs, from preferences and the token.
pub(crate) fn network_options(preferences: &Preferences, token: Option<String>) -> NetworkOptions {
	NetworkOptions {
		proxy: preferences.network.proxy.clone(),
		timeout_seconds: preferences.network.timeout_seconds,
		github_token: token,
	}
}

fn token_from_environment() -> Option<String> {
	std::env::var(TOKEN_VARIABLE)
		.ok()
		.map(|value| clean_token(&value))
		.filter(|value| !value.is_empty())
}

/// Remembered GitHub answers, kept in the launcher's database.
#[derive(Clone)]
pub struct StoredResponses(pub(crate) std::sync::Arc<Database>);

impl ResponseStore for StoredResponses {
	async fn load(&self, url: &Url) -> Option<StoredResponse> {
		let cached = self
			.0
			.cached_response(url.as_str().to_owned())
			.await
			.ok()??;

		Some(StoredResponse {
			etag: cached.etag,
			body: cached.body,
		})
	}

	async fn save(&self, url: &Url, response: StoredResponse) {
		if let Err(error) = self
			.0
			.store_response(
				url.as_str().to_owned(),
				response.etag,
				response.body,
				now_unix(),
			)
			.await
		{
			tracing::debug!(%error, "could not remember an answer");
		}
	}
}

/// Where GitHub says the limit stands, as of one lookup. Asking costs nothing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RateLimit {
	pub limit: u64,
	pub remaining: u64,
	/// Unix seconds when the window resets.
	pub resets_at: i64,
	/// The lookup carried a token, so the limit is the signed-in one.
	pub signed_in: bool,
}

#[derive(Deserialize)]
struct RawLimits {
	resources: RawResources,
}

#[derive(Deserialize)]
struct RawResources {
	core: RawCore,
}

#[derive(Deserialize)]
struct RawCore {
	limit: u64,
	remaining: u64,
	reset: i64,
}

impl Engine {
	/// The saved preferences.
	pub async fn preferences(&self) -> Preferences {
		read_preferences(&self.database).await
	}

	/// Saves preferences and applies them now: formats, and the network.
	pub async fn set_preferences(&self, preferences: Preferences) -> Result<(), LauncherError> {
		let preferences = preferences.sanitized();
		let text = serde_json::to_string(&preferences).map_err(|error| {
			LauncherError::new(
				ErrorCode::InternalInvariant,
				"The settings could not be saved.",
			)
			.with_source(error)
		})?;

		self.database.set_setting(PREFERENCES_KEY, text).await?;
		self.apply_preferences(&preferences).await;

		Ok(())
	}

	/// Puts preferences into effect: the clock and date format, and the
	/// network settings, for every holder of the client.
	pub(crate) async fn apply_preferences(&self, preferences: &Preferences) {
		local_time::configure(preferences.time_format, preferences.date_format);

		let token = self.effective_token().await;

		if let Err(error) = self.http.reconfigure(&network_options(preferences, token)) {
			tracing::warn!(%error, "the network settings could not be applied");
		}
	}

	/// The token in use: the environment's, else the saved one.
	pub(crate) async fn effective_token(&self) -> Option<String> {
		if let Some(token) = token_from_environment() {
			return Some(token);
		}

		if !self.preferences().await.network.token_saved {
			return None;
		}

		let secrets = self.secrets.clone();

		tokio::task::spawn_blocking(move || secrets.get())
			.await
			.ok()?
			.ok()
			.flatten()
			.filter(|token| !token.is_empty())
	}

	/// Whether a token is set and where it comes from. Never the token.
	pub async fn token_state(&self) -> TokenState {
		if token_from_environment().is_some() {
			return TokenState::FromEnvironment;
		}

		if !self.preferences().await.network.token_saved {
			return TokenState::Absent;
		}

		let secrets = self.secrets.clone();

		match tokio::task::spawn_blocking(move || secrets.get()).await {
			Ok(Ok(Some(token))) if !token.is_empty() => TokenState::Stored,
			Ok(Ok(_)) => TokenState::Absent,
			Ok(Err(error)) => TokenState::Unavailable(error.to_string()),
			Err(_) => TokenState::Unavailable("The credential store did not answer.".into()),
		}
	}

	/// Saves a token in the system's credential store, after a plain check
	/// that it looks like one.
	pub async fn set_github_token(&self, pasted: &str) -> Result<(), LauncherError> {
		let token = clean_token(pasted);

		if !token_looks_valid(&token) {
			return Err(LauncherError::new(
				ErrorCode::InvalidSourceUrl,
				"That does not look like a GitHub token. Paste the whole token and nothing else.",
			));
		}

		let secrets = self.secrets.clone();
		let to_save = token.clone();

		tokio::task::spawn_blocking(move || secrets.set(&to_save))
			.await
			.map_err(storage_unavailable)?
			.map_err(|error| {
				LauncherError::new(
					ErrorCode::StorageUnavailable,
					"The system's credential store would not keep the token.",
				)
				.with_source(std::io::Error::other(error.to_string()))
			})?;

		let mut preferences = self.preferences().await;

		preferences.network.token_saved = true;
		self.set_preferences(preferences).await
	}

	pub async fn clear_github_token(&self) -> Result<(), LauncherError> {
		let secrets = self.secrets.clone();

		tokio::task::spawn_blocking(move || secrets.clear())
			.await
			.map_err(storage_unavailable)?
			.map_err(|error| {
				LauncherError::new(
					ErrorCode::StorageUnavailable,
					"The saved token could not be removed.",
				)
				.with_source(std::io::Error::other(error.to_string()))
			})?;

		let mut preferences = self.preferences().await;

		preferences.network.token_saved = false;
		self.set_preferences(preferences).await
	}

	/// Asks GitHub where the rate limit stands. This lookup is free.
	pub async fn rate_limit(&self) -> Result<RateLimit, LauncherError> {
		let url = Url::parse("https://api.github.com/rate_limit").map_err(|_| internal())?;
		let response = self
			.http
			.fetch(
				url,
				&MetadataRequest {
					accept: Some("application/vnd.github+json"),
					..MetadataRequest::default()
				},
				64 * 1024,
			)
			.await?;

		if response.status != 200 {
			return Err(LauncherError::new(
				ErrorCode::NetworkRequestFailed,
				"GitHub could not say where the limit stands.",
			));
		}

		let raw: RawLimits = serde_json::from_slice(&response.body).map_err(|_| {
			LauncherError::new(
				ErrorCode::ProviderResponseMalformed,
				"GitHub's answer could not be read.",
			)
		})?;

		Ok(RateLimit {
			limit: raw.resources.core.limit,
			remaining: raw.resources.core.remaining,
			resets_at: raw.resources.core.reset,
			signed_in: self.effective_token().await.is_some(),
		})
	}

	/// A provider that revalidates its answers instead of asking again.
	pub(crate) fn provider(&self) -> GithubProvider<CachedTransport<HttpClient, StoredResponses>> {
		GithubProvider::new(CachedTransport::new(
			self.http.clone(),
			StoredResponses(self.database.clone()),
		))
	}

	/// Forgets every remembered GitHub answer. The next check asks afresh.
	pub async fn clear_response_cache(&self) -> Result<usize, LauncherError> {
		self.database.clear_responses().await
	}
}

fn internal() -> LauncherError {
	LauncherError::new(
		ErrorCode::InternalInvariant,
		"The launcher hit an unexpected problem.",
	)
}

fn storage_unavailable(error: tokio::task::JoinError) -> LauncherError {
	LauncherError::new(
		ErrorCode::StorageUnavailable,
		"The credential store did not answer.",
	)
	.with_source(error)
}

#[cfg(test)]
mod tests {
	use std::sync::Arc;

	use launcher_model::local_time::{DateFormat, TimeFormat};
	use launcher_model::{Preferences, Proxy};

	use super::*;
	use crate::{DataDirectory, Engine, EngineConfig, MemorySecret, SecretStore};

	async fn engine(secrets: MemorySecret) -> (tempfile::TempDir, Engine) {
		let directory = tempfile::tempdir().unwrap();
		let config = EngineConfig {
			secrets: Some(Arc::new(secrets)),
			..EngineConfig::production()
		};
		let engine = Engine::open_with(&DataDirectory::at(directory.path().join("data")), config)
			.await
			.unwrap();

		(directory, engine)
	}

	#[tokio::test]
	async fn preferences_default_then_save_and_load_again() {
		let (_directory, engine) = engine(MemorySecret::default()).await;

		assert_eq!(engine.preferences().await, Preferences::default());

		let mut changed = Preferences {
			ui_scale: 125,
			time_format: TimeFormat::Hour24,
			date_format: DateFormat::Iso,
			..Preferences::default()
		};

		changed.network.proxy = Proxy::Direct;
		changed.layout.sidebar_collapsed = true;

		engine.set_preferences(changed.clone()).await.unwrap();

		assert_eq!(engine.preferences().await, changed);
	}

	#[tokio::test]
	async fn unusable_values_are_repaired_on_the_way_in() {
		let (_directory, engine) = engine(MemorySecret::default()).await;
		let mut bad = Preferences {
			ui_scale: 9_999,
			..Preferences::default()
		};

		bad.network.proxy = Proxy::Custom("http://user:pw@host".into());

		engine.set_preferences(bad).await.unwrap();

		let saved = engine.preferences().await;

		assert_eq!(saved.ui_scale, 200);
		assert_eq!(saved.network.proxy, Proxy::System);
	}

	#[tokio::test]
	async fn a_corrupt_saved_document_falls_back_instead_of_failing() {
		let (_directory, engine) = engine(MemorySecret::default()).await;

		engine
			.database
			.set_setting(PREFERENCES_KEY, "{not json".into())
			.await
			.unwrap();

		assert_eq!(engine.preferences().await, Preferences::default());
	}

	#[tokio::test]
	async fn the_token_is_kept_in_the_secret_store_and_never_in_settings() {
		let secrets = MemorySecret::default();
		let (_directory, engine) = engine(secrets.clone()).await;
		let token = "ghp_abcdefghijklmnopqrstuvwxyz0123456789";

		assert_eq!(engine.token_state().await, TokenState::Absent);

		engine
			.set_github_token(&format!("  {token}\n"))
			.await
			.unwrap();

		assert_eq!(engine.token_state().await, TokenState::Stored);
		assert_eq!(secrets.get().unwrap().as_deref(), Some(token));

		let saved = engine
			.database
			.setting(PREFERENCES_KEY)
			.await
			.unwrap()
			.unwrap();

		assert!(
			!saved.contains(token),
			"the token must never reach the database"
		);
		assert!(engine.preferences().await.network.token_saved);

		engine.clear_github_token().await.unwrap();

		assert_eq!(engine.token_state().await, TokenState::Absent);
		assert_eq!(secrets.get().unwrap(), None);
		assert!(!engine.preferences().await.network.token_saved);
	}

	#[tokio::test]
	async fn something_that_is_not_a_token_is_refused_before_it_is_stored() {
		let secrets = MemorySecret::default();
		let (_directory, engine) = engine(secrets.clone()).await;

		let error = engine.set_github_token("not a token").await.unwrap_err();

		assert_eq!(error.code(), ErrorCode::InvalidSourceUrl);
		assert_eq!(secrets.get().unwrap(), None);
	}

	#[tokio::test]
	async fn the_credential_store_is_never_touched_unless_a_token_was_saved() {
		struct Tripwire;

		impl crate::SecretStore for Tripwire {
			fn get(&self) -> Result<Option<String>, crate::SecretError> {
				panic!("the credential store must not be read when no token was ever saved");
			}

			fn set(&self, _: &str) -> Result<(), crate::SecretError> {
				Ok(())
			}

			fn clear(&self) -> Result<(), crate::SecretError> {
				Ok(())
			}
		}

		let directory = tempfile::tempdir().unwrap();
		let config = EngineConfig {
			secrets: Some(Arc::new(Tripwire)),
			..EngineConfig::production()
		};
		let engine = Engine::open_with(&DataDirectory::at(directory.path().join("data")), config)
			.await
			.unwrap();

		assert_eq!(engine.token_state().await, TokenState::Absent);
		assert_eq!(engine.effective_token().await, None);
	}
}
