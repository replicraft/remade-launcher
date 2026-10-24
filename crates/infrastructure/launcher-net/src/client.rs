use std::time::Duration;

use futures_util::StreamExt;
use launcher_model::{ErrorCode, LauncherError, product};
use reqwest::header::{ACCEPT, HeaderMap, HeaderName, HeaderValue, IF_NONE_MATCH};
use reqwest::{Client, Url, redirect};

use crate::bounded::BoundedBuffer;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const IDLE_CONNECTION_TIMEOUT: Duration = Duration::from_secs(90);
const MAX_REDIRECTS: usize = 5;
/// Longest a transfer may sit idle before it fails.
const READ_TIMEOUT: Duration = Duration::from_secs(30);
/// Hard ceiling for one artifact download.
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(6 * 60 * 60);

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
			.redirect(redirect_policy())
			.read_timeout(READ_TIMEOUT)
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

/// Follows a bounded number of redirects, and only to public HTTPS hosts.
///
/// GitHub serves release assets from another host, so redirects must work,
/// but a redirect must never reach a loopback, private, or link-local address
/// or a bare `localhost`. This checks the host text. It does not stop DNS
/// rebinding, which needs resolver-level control.
fn redirect_policy() -> redirect::Policy {
	redirect::Policy::custom(|attempt| {
		if attempt.previous().len() >= MAX_REDIRECTS {
			return attempt.error("too many redirects");
		}

		if is_public_https(attempt.url()) {
			attempt.follow()
		} else {
			attempt.error("redirect to a non-public destination")
		}
	})
}

pub(crate) fn is_public_https(url: &Url) -> bool {
	use std::net::IpAddr;

	if url.scheme() != "https" {
		return false;
	}

	match url.host() {
		Some(url::Host::Domain(domain)) => {
			let domain = domain.to_ascii_lowercase();

			!(domain == "localhost" || domain.ends_with(".localhost") || domain.ends_with(".local") || domain.ends_with(".internal"))
		}
		Some(url::Host::Ipv4(address)) => is_public_ip(IpAddr::V4(address)),
		Some(url::Host::Ipv6(address)) => is_public_ip(IpAddr::V6(address)),
		None => false,
	}
}

fn is_public_ip(address: std::net::IpAddr) -> bool {
	match address {
		std::net::IpAddr::V4(v4) => {
			!(v4.is_loopback()
				|| v4.is_private()
				|| v4.is_link_local()
				|| v4.is_unspecified()
				|| v4.is_broadcast()
				|| v4.is_multicast()
				|| v4.octets()[0] == 0
				// Carrier-grade NAT, 100.64.0.0/10.
				|| (v4.octets()[0] == 100 && (64..128).contains(&v4.octets()[1])))
		}
		std::net::IpAddr::V6(v6) => {
			!(v6.is_loopback()
				|| v6.is_unspecified()
				|| v6.is_multicast()
				// Unique local fc00::/7 and link-local fe80::/10.
				|| (v6.segments()[0] & 0xfe00) == 0xfc00
				|| (v6.segments()[0] & 0xffc0) == 0xfe80
				|| v6.to_ipv4_mapped().is_some_and(|mapped| !is_public_ip(std::net::IpAddr::V4(mapped))))
		}
	}
}

/// What a finished download produced.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Downloaded {
	pub size: u64,
	/// Lowercase hex SHA-256 of the bytes written.
	pub sha256: String,
}

impl HttpClient {
	/// Streams `url` into `file`, hashing as it goes.
	///
	/// Fails if the body exceeds `max_bytes`. The caller owns `file`: it is
	/// flushed and synced here, but never deleted, so the caller decides
	/// what an incomplete file means.
	pub async fn download_to(
		&self,
		url: Url,
		file: &mut tokio::fs::File,
		max_bytes: u64,
		mut progress: impl FnMut(u64, Option<u64>) + Send,
	) -> Result<Downloaded, LauncherError> {
		use sha2::{Digest, Sha256};
		use tokio::io::AsyncWriteExt;

		let io_failed = |error: std::io::Error| {
			LauncherError::new(ErrorCode::StorageUnavailable, "The download could not be saved to disk.")
				.with_source(error)
		};

		let response = self
			.inner
			.get(url)
			.timeout(DOWNLOAD_TIMEOUT)
			.send()
			.await
			.and_then(|response| response.error_for_status())
			.map_err(request_failed)?;

		let total = response.content_length();

		if total.is_some_and(|total| total > max_bytes) {
			return Err(LauncherError::new(
				ErrorCode::NetworkResponseTooLarge,
				"The file is larger than the launcher allows.",
			));
		}

		let mut hasher = Sha256::new();
		let mut written = 0u64;
		let mut stream = response.bytes_stream();

		while let Some(chunk) = stream.next().await {
			let chunk = chunk.map_err(request_failed)?;

			written += chunk.len() as u64;

			if written > max_bytes {
				return Err(LauncherError::new(
					ErrorCode::NetworkResponseTooLarge,
					"The file is larger than the launcher allows.",
				));
			}

			hasher.update(&chunk);
			file.write_all(&chunk).await.map_err(io_failed)?;
			progress(written, total);
		}

		file.flush().await.map_err(io_failed)?;
		file.sync_all().await.map_err(io_failed)?;

		Ok(Downloaded {
			size: written,
			sha256: hex::encode(hasher.finalize()),
		})
	}
}

/// A response whose status the caller interprets. Unlike
/// [`HttpClient::get_bounded`], a 404 or 429 is data here, because a provider
/// needs to tell "gone" from "rate limited" from "denied".
#[derive(Clone, Debug)]
pub struct HttpResponse {
	pub status: u16,
	pub etag: Option<String>,
	pub retry_after_seconds: Option<u64>,
	pub rate_limit_remaining: Option<u64>,
	/// Unix seconds at which the rate-limit window resets.
	pub rate_limit_reset: Option<u64>,
	/// The `rel="next"` URL from a `Link` header, if any.
	pub next_page: Option<Url>,
	pub body: Vec<u8>,
}

/// Extra request details for a metadata call.
#[derive(Clone, Debug, Default)]
pub struct MetadataRequest {
	pub accept: Option<&'static str>,
	/// Sent as `If-None-Match` to revalidate a cached copy.
	pub etag: Option<String>,
	/// Additional fixed headers, such as an API version.
	pub headers: Vec<(&'static str, &'static str)>,
}

impl HttpClient {
	/// Fetches `url` and returns the response whatever its status, with the
	/// body bounded to `max_bytes`.
	pub async fn fetch(
		&self,
		url: Url,
		request: &MetadataRequest,
		max_bytes: usize,
	) -> Result<HttpResponse, LauncherError> {
		let mut headers = HeaderMap::new();

		if let Some(accept) = request.accept {
			headers.insert(ACCEPT, HeaderValue::from_static(accept));
		}

		if let Some(etag) = &request.etag
			&& let Ok(value) = HeaderValue::from_str(etag)
		{
			headers.insert(IF_NONE_MATCH, value);
		}

		for (name, value) in &request.headers {
			headers.insert(
				HeaderName::from_static(name),
				HeaderValue::from_static(value),
			);
		}

		let response = self
			.inner
			.get(url)
			.headers(headers)
			.send()
			.await
			.map_err(request_failed)?;

		let status = response.status().as_u16();
		let header = |name: &str| {
			response
				.headers()
				.get(name)
				.and_then(|value| value.to_str().ok())
				.map(str::to_owned)
		};

		let etag = header("etag");
		let retry_after_seconds = header("retry-after").and_then(|value| value.parse().ok());
		let rate_limit_remaining =
			header("x-ratelimit-remaining").and_then(|value| value.parse().ok());
		let rate_limit_reset = header("x-ratelimit-reset").and_then(|value| value.parse().ok());
		let next_page = header("link").and_then(|link| next_link(&link));

		let mut buffer = BoundedBuffer::new(max_bytes, response.content_length())?;
		let mut stream = response.bytes_stream();

		while let Some(chunk) = stream.next().await {
			buffer.push(&chunk.map_err(request_failed)?)?;
		}

		Ok(HttpResponse {
			status,
			etag,
			retry_after_seconds,
			rate_limit_remaining,
			rate_limit_reset,
			next_page,
			body: buffer.into_vec(),
		})
	}
}

/// Extracts the `rel="next"` target from an RFC 8288 `Link` header.
fn next_link(header: &str) -> Option<Url> {
	header.split(',').find_map(|part| {
		let (target, parameters) = part.split_once(';')?;

		if !parameters
			.split(';')
			.any(|p| p.trim().replace(' ', "") == "rel=\"next\"")
		{
			return None;
		}

		let target = target.trim().strip_prefix('<')?.strip_suffix('>')?;

		Url::parse(target).ok()
	})
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
	fn only_public_https_destinations_are_followed() {
		for allowed in [
			"https://github.com/a",
			"https://objects.githubusercontent.com/x",
			"https://8.8.8.8/x",
		] {
			assert!(is_public_https(&Url::parse(allowed).unwrap()), "{allowed}");
		}

		for refused in [
			"http://github.com/a",
			"https://localhost/a",
			"https://foo.localhost/a",
			"https://printer.local/a",
			"https://127.0.0.1/a",
			"https://10.0.0.5/a",
			"https://192.168.1.1/a",
			"https://172.16.0.1/a",
			"https://169.254.169.254/latest/meta-data",
			"https://100.64.0.1/a",
			"https://0.0.0.0/a",
			"https://[::1]/a",
			"https://[fd00::1]/a",
			"https://[fe80::1]/a",
			"https://[::ffff:127.0.0.1]/a",
			"ftp://github.com/a",
		] {
			assert!(!is_public_https(&Url::parse(refused).unwrap()), "{refused}");
		}
	}

	#[test]
	fn finds_the_next_page_link() {
		let header = r#"<https://api.github.com/x?page=2>; rel="next", <https://api.github.com/x?page=9>; rel="last""#;

		assert_eq!(
			next_link(header).unwrap().as_str(),
			"https://api.github.com/x?page=2"
		);
		assert!(next_link(r#"<https://a/b>; rel="last""#).is_none());
		assert!(next_link("garbage").is_none());
	}

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
