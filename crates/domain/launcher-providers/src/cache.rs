//! Revalidating GitHub answers instead of asking again.
//!
//! GitHub does not count a request against the rate limit when it answers
//! "not modified". So every answer is remembered with its validator, and the
//! next lookup of the same address sends it back. When nothing changed, the
//! launcher gets the remembered body for free; when something did, it gets the
//! new one. Checking a long library of apps then costs almost nothing.

use std::future::Future;

use launcher_model::LauncherError;
use launcher_net::{HttpResponse, MetadataRequest, Url};

use crate::transport::Transport;

/// One remembered answer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StoredResponse {
	pub etag: String,
	pub body: Vec<u8>,
}

/// Where answers are remembered. Failing to read or write is never an error
/// for a lookup: it just means the lookup is not cached.
pub trait ResponseStore: Send + Sync {
	fn load(&self, url: &Url) -> impl Future<Output = Option<StoredResponse>> + Send;

	fn save(&self, url: &Url, response: StoredResponse) -> impl Future<Output = ()> + Send;
}

/// A [`Transport`] that revalidates through a [`ResponseStore`].
#[derive(Clone)]
pub struct CachedTransport<T, S> {
	inner: T,
	store: S,
}

impl<T, S> CachedTransport<T, S> {
	pub fn new(inner: T, store: S) -> Self {
		Self { inner, store }
	}
}

/// Only GitHub's API gives validators worth keeping.
fn cacheable(url: &Url) -> bool {
	url.scheme() == "https" && url.host_str() == Some("api.github.com")
}

impl<T: Transport, S: ResponseStore> Transport for CachedTransport<T, S> {
	async fn get(
		&self,
		url: Url,
		mut request: MetadataRequest,
		max_bytes: usize,
	) -> Result<HttpResponse, LauncherError> {
		if !cacheable(&url) {
			return self.inner.get(url, request, max_bytes).await;
		}

		let remembered = match request.etag {
			Some(_) => None,
			None => self.store.load(&url).await,
		};

		if let Some(stored) = &remembered {
			request.etag = Some(stored.etag.clone());
		}

		let mut response = self.inner.get(url.clone(), request, max_bytes).await?;

		match (response.status, remembered) {
			// Unchanged: hand back what was remembered, as if it were fresh.
			(304, Some(stored)) => {
				response.status = 200;
				response.etag = Some(stored.etag);
				response.body = stored.body;
			}
			(200, _) => {
				if let Some(etag) = response.etag.clone() {
					self.store
						.save(
							&url,
							StoredResponse {
								etag,
								body: response.body.clone(),
							},
						)
						.await;
				}
			}
			_ => {}
		}

		Ok(response)
	}
}

#[cfg(test)]
mod tests {
	use std::collections::HashMap;
	use std::sync::{Arc, Mutex};

	use super::*;

	#[derive(Clone, Default)]
	struct Memory(Arc<Mutex<HashMap<String, StoredResponse>>>);

	impl ResponseStore for Memory {
		async fn load(&self, url: &Url) -> Option<StoredResponse> {
			self.0.lock().unwrap().get(url.as_str()).cloned()
		}

		async fn save(&self, url: &Url, response: StoredResponse) {
			self.0
				.lock()
				.unwrap()
				.insert(url.as_str().to_owned(), response);
		}
	}

	/// Answers from a script and records what etag each request carried.
	#[derive(Clone)]
	struct Scripted {
		answers: Arc<Mutex<Vec<HttpResponse>>>,
		sent: Arc<Mutex<Vec<Option<String>>>>,
	}

	impl Scripted {
		fn new(answers: Vec<HttpResponse>) -> Self {
			Self {
				answers: Arc::new(Mutex::new(answers)),
				sent: Arc::default(),
			}
		}
	}

	impl Transport for Scripted {
		async fn get(
			&self,
			_url: Url,
			request: MetadataRequest,
			_max: usize,
		) -> Result<HttpResponse, LauncherError> {
			self.sent.lock().unwrap().push(request.etag);

			Ok(self.answers.lock().unwrap().remove(0))
		}
	}

	fn response(status: u16, etag: Option<&str>, body: &str) -> HttpResponse {
		HttpResponse {
			status,
			etag: etag.map(str::to_owned),
			retry_after_seconds: None,
			rate_limit_remaining: Some(42),
			rate_limit_reset: None,
			next_page: None,
			body: body.as_bytes().to_vec(),
		}
	}

	fn api(path: &str) -> Url {
		Url::parse(&format!("https://api.github.com/{path}")).unwrap()
	}

	#[tokio::test]
	async fn a_second_lookup_sends_the_validator_and_reuses_the_body_when_unchanged() {
		let inner = Scripted::new(vec![
			response(200, Some("\"v1\""), "first"),
			response(304, None, ""),
		]);
		let transport = CachedTransport::new(inner.clone(), Memory::default());

		let first = transport
			.get(api("repos/o/r/releases"), MetadataRequest::default(), 10)
			.await
			.unwrap();
		let second = transport
			.get(api("repos/o/r/releases"), MetadataRequest::default(), 10)
			.await
			.unwrap();

		assert_eq!((first.status, first.body.as_slice()), (200, &b"first"[..]));
		assert_eq!(
			(second.status, second.body.as_slice()),
			(200, &b"first"[..])
		);
		assert_eq!(second.etag.as_deref(), Some("\"v1\""));
		assert_eq!(
			second.rate_limit_remaining,
			Some(42),
			"the live rate limit still comes through"
		);
		assert_eq!(
			*inner.sent.lock().unwrap(),
			[None, Some("\"v1\"".to_owned())]
		);
	}

	#[tokio::test]
	async fn a_changed_answer_replaces_the_remembered_one() {
		let inner = Scripted::new(vec![
			response(200, Some("\"v1\""), "old"),
			response(200, Some("\"v2\""), "new"),
			response(304, None, ""),
		]);
		let transport = CachedTransport::new(inner, Memory::default());

		for _ in 0..2 {
			transport
				.get(api("x"), MetadataRequest::default(), 10)
				.await
				.unwrap();
		}

		let third = transport
			.get(api("x"), MetadataRequest::default(), 10)
			.await
			.unwrap();

		assert_eq!(third.body, b"new");
	}

	#[tokio::test]
	async fn errors_and_other_hosts_are_left_alone() {
		let inner = Scripted::new(vec![
			response(404, None, ""),
			response(200, Some("\"e\""), "other"),
		]);
		let store = Memory::default();
		let transport = CachedTransport::new(inner.clone(), store.clone());

		assert_eq!(
			transport
				.get(api("missing"), MetadataRequest::default(), 10)
				.await
				.unwrap()
				.status,
			404
		);

		let elsewhere = Url::parse("https://example.com/data").unwrap();

		transport
			.get(elsewhere, MetadataRequest::default(), 10)
			.await
			.unwrap();

		assert!(store.0.lock().unwrap().is_empty(), "neither was remembered");
	}

	#[tokio::test]
	async fn a_rate_limited_answer_is_never_replaced_with_old_data() {
		let inner = Scripted::new(vec![
			response(200, Some("\"v1\""), "old"),
			response(403, None, ""),
		]);
		let transport = CachedTransport::new(inner, Memory::default());

		transport
			.get(api("x"), MetadataRequest::default(), 10)
			.await
			.unwrap();

		let limited = transport
			.get(api("x"), MetadataRequest::default(), 10)
			.await
			.unwrap();

		assert_eq!(
			limited.status, 403,
			"a failed check must stay a failed check"
		);
	}
}
