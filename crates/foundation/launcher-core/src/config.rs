use std::sync::Arc;

use launcher_catalog::{KeyRegistry, SUPPORTED_CAPABILITIES};
use launcher_model::product;
use launcher_net::Url;

use crate::credentials::SecretStore;
use crate::lifecycle::ArtifactSource;
use crate::self_update::ReleaseConfig;

/// Where the catalog lives and which keys may sign it.
#[derive(Clone)]
pub struct CatalogConfig {
	pub feed_id: String,
	pub manifest_url: Url,
	pub envelope_url: Url,
	pub registry: Arc<KeyRegistry>,
	pub supported_capabilities: &'static [&'static str],
}

impl CatalogConfig {
	/// The official Remade catalog, trusted through the compiled-in registry.
	pub fn production() -> Self {
		let base = |path: &str| {
			Url::parse(&format!("https://remade.wiki/data/{path}"))
				.expect("the catalog URLs are valid")
		};

		Self {
			feed_id: product::CATALOG_FEED_ID.to_owned(),
			manifest_url: base("launcher-catalog.json"),
			envelope_url: base("launcher-catalog.sig"),
			registry: Arc::new(KeyRegistry::production()),
			supported_capabilities: SUPPORTED_CAPABILITIES,
		}
	}
}

#[derive(Clone)]
pub struct EngineConfig {
	pub catalog: CatalogConfig,
	/// Where artifact bytes come from. `None` downloads over HTTPS; tests
	/// supply local files.
	pub artifacts: Option<Arc<dyn ArtifactSource>>,
	/// Where the GitHub token is kept. `None` uses the system's credential
	/// store; tests supply memory.
	pub secrets: Option<Arc<dyn SecretStore>>,
	/// Where the launcher's own releases come from.
	pub releases: ReleaseConfig,
}

impl EngineConfig {
	pub fn production() -> Self {
		Self {
			catalog: CatalogConfig::production(),
			artifacts: None,
			secrets: None,
			releases: ReleaseConfig::production(),
		}
	}
}
