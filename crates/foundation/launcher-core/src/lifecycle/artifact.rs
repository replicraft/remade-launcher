use std::future::Future;
use std::pin::Pin;

use launcher_model::LauncherError;
use launcher_net::{Downloaded, HttpClient, Url};

type Boxed<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Where artifact bytes come from. Production downloads over HTTPS; tests
/// copy local files, so they never touch the network.
pub trait ArtifactSource: Send + Sync {
	fn fetch<'a>(
		&'a self,
		url: Url,
		file: &'a mut tokio::fs::File,
		max_bytes: u64,
		progress: &'a mut (dyn FnMut(u64, Option<u64>) + Send),
	) -> Boxed<'a, Result<Downloaded, LauncherError>>;
}

pub struct HttpArtifacts(pub HttpClient);

impl ArtifactSource for HttpArtifacts {
	fn fetch<'a>(
		&'a self,
		url: Url,
		file: &'a mut tokio::fs::File,
		max_bytes: u64,
		progress: &'a mut (dyn FnMut(u64, Option<u64>) + Send),
	) -> Boxed<'a, Result<Downloaded, LauncherError>> {
		Box::pin(async move { self.0.download_to(url, file, max_bytes, progress).await })
	}
}
