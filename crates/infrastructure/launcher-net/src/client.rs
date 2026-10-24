use std::time::Duration;

use futures_util::StreamExt;
use launcher_model::{ErrorCode, LauncherError, product};
use reqwest::{Client, Url, redirect};

use crate::bounded::BoundedBuffer;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const IDLE_CONNECTION_TIMEOUT: Duration = Duration::from_secs(90);
const MAX_REDIRECTS: usize = 5;

/// A cheaply cloneable HTTPS client.
#[derive(Clone)]
pub struct HttpClient {
	inner: Client,
}

impl HttpClient {
	pub fn new() -> Result<Self, LauncherError> {
		let inner = Client::builder()
			.user_agent(product::user_agent())
			.https_only(true)
			.redirect(redirect::Policy::limited(MAX_REDIRECTS))
			.connect_timeout(CONNECT_TIMEOUT)
			.timeout(REQUEST_TIMEOUT)
			.pool_idle_timeout(IDLE_CONNECTION_TIMEOUT)
			.tcp_nodelay(true)
			.http2_adaptive_window(true)
			.build()
			.map_err(request_failed)?;

		Ok(Self { inner })
	}

	/// Downloads `url` into memory, failing if the body exceeds `max_bytes`.
	///
	/// This is for small documents such as catalogs and release metadata.
	/// Large artifacts will stream to disk through a separate path.
	pub async fn get_bounded(&self, url: Url, max_bytes: usize) -> Result<Vec<u8>, LauncherError> {
		let response = self
			.inner
			.get(url)
			.send()
			.await
			.and_then(|response| response.error_for_status())
			.map_err(request_failed)?;

		let mut buffer = BoundedBuffer::new(max_bytes, response.content_length())?;
		let mut stream = response.bytes_stream();

		while let Some(chunk) = stream.next().await {
			buffer.push(&chunk.map_err(request_failed)?)?;
		}

		Ok(buffer.into_vec())
	}
}

/// The error text from `reqwest` includes the full URL, which can be private,
/// so only the stable message is user-facing and the cause stays in the chain.
fn request_failed(error: reqwest::Error) -> LauncherError {
	LauncherError::new(
		ErrorCode::NetworkRequestFailed,
		"The request could not be completed.",
	)
	.with_source(error.without_url())
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn builds() {
		HttpClient::new().unwrap();
	}

	#[tokio::test]
	async fn refuses_plain_http() {
		let client = HttpClient::new().unwrap();
		let url = Url::parse("http://example.invalid/catalog.json").unwrap();

		let error = client.get_bounded(url, 1024).await.unwrap_err();

		assert_eq!(error.code(), ErrorCode::NetworkRequestFailed);
	}
}
