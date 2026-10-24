use std::future::Future;

use launcher_model::LauncherError;
use launcher_net::{HttpClient, HttpResponse, MetadataRequest, Url};

/// The one thing a provider needs from the network. Tests supply a fake, so
/// they never touch the public GitHub API.
pub trait Transport: Send + Sync {
	fn get(
		&self,
		url: Url,
		request: MetadataRequest,
		max_bytes: usize,
	) -> impl Future<Output = Result<HttpResponse, LauncherError>> + Send;
}

impl Transport for HttpClient {
	async fn get(
		&self,
		url: Url,
		request: MetadataRequest,
		max_bytes: usize,
	) -> Result<HttpResponse, LauncherError> {
		self.fetch(url, &request, max_bytes).await
	}
}
