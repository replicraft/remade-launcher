//! GitHub Releases, through the documented REST API.

use launcher_model::{ErrorCode, LauncherError};
use launcher_net::{HttpResponse, MetadataRequest, Url};
use serde::Deserialize;

use crate::model::{Asset, Release, ReleaseObservation, RepositoryIdentity};
use crate::transport::Transport;

const API: &str = "https://api.github.com";
const ACCEPT: &str = "application/vnd.github+json";
const API_VERSION: &str = "2022-11-28";

/// A bounded response keeps a hostile or broken server from filling memory.
const MAX_RESPONSE_BYTES: usize = 8 * 1024 * 1024;
const RELEASES_PER_PAGE: usize = 100;
/// Three pages is 300 releases. Past that, discovery is reported incomplete.
const MAX_PAGES: usize = 3;
const MAX_NOTES_CHARS: usize = 20_000;

pub struct GithubProvider<T> {
	transport: T,
	api_base: Url,
}

impl<T: Transport> GithubProvider<T> {
	pub fn new(transport: T) -> Self {
		Self {
			transport,
			api_base: Url::parse(API).expect("the API base is a valid URL"),
		}
	}

	/// Looks a repository up by name. Use this for a user-supplied link, then
	/// pin the returned numeric IDs.
	pub async fn repository_by_name(
		&self,
		owner: &str,
		name: &str,
	) -> Result<RepositoryIdentity, LauncherError> {
		let url = self.url(&["repos", owner, name]);

		self.repository(url).await
	}

	/// Looks a repository up by its stable ID, for catalog entries.
	pub async fn repository_by_id(&self, id: u64) -> Result<RepositoryIdentity, LauncherError> {
		let url = self.url(&["repositories", &id.to_string()]);

		self.repository(url).await
	}

	/// Lists published releases, newest first, for a repository ID.
	pub async fn releases(
		&self,
		repository: RepositoryIdentity,
	) -> Result<ReleaseObservation, LauncherError> {
		let mut url = self.url(&[
			"repositories",
			&repository.repository_id.to_string(),
			"releases",
		]);
		url.query_pairs_mut()
			.append_pair("per_page", &RELEASES_PER_PAGE.to_string());

		let mut releases = Vec::new();
		let mut next = Some(url);
		let mut pages = 0;

		while let Some(page_url) = next.take() {
			if pages == MAX_PAGES {
				return Ok(ReleaseObservation {
					repository,
					releases,
					complete: false,
				});
			}

			pages += 1;

			let response = self.get(page_url).await?;
			let page: Vec<RawRelease> = parse(&response)?;

			releases.extend(
				page.into_iter()
					.filter(|release| !release.draft)
					.map(Release::from),
			);

			next = response.next_page.filter(|url| self.same_api(url));
		}

		Ok(ReleaseObservation {
			repository,
			releases,
			complete: true,
		})
	}

	async fn repository(&self, url: Url) -> Result<RepositoryIdentity, LauncherError> {
		let response = self.get(url).await?;
		let raw: RawRepository = parse(&response)?;

		if raw.private {
			// A private repository would not normally answer anonymously, but
			// never treat one as a public source if it does.
			return Err(LauncherError::new(
				ErrorCode::ProviderDenied,
				"Only public repositories are supported.",
			));
		}

		Ok(RepositoryIdentity {
			repository_id: raw.id,
			owner_id: raw.owner.id,
			owner: raw.owner.login,
			name: raw.name,
		})
	}

	async fn get(&self, url: Url) -> Result<HttpResponse, LauncherError> {
		let request = MetadataRequest {
			accept: Some(ACCEPT),
			etag: None,
			headers: vec![("x-github-api-version", API_VERSION)],
		};

		let response = self.transport.get(url, request, MAX_RESPONSE_BYTES).await?;

		match response.status {
			200 => Ok(response),
			404 => Err(LauncherError::new(
				ErrorCode::ProviderNotFound,
				"That repository or release was not found. It may be private or removed.",
			)),
			403 | 429 if is_rate_limited(&response) => Err(LauncherError::new(
				ErrorCode::ProviderRateLimited,
				"GitHub's request limit was reached. Try again later.",
			)),
			401 | 403 => Err(LauncherError::new(
				ErrorCode::ProviderDenied,
				"GitHub refused the request.",
			)),
			_ => Err(LauncherError::new(
				ErrorCode::NetworkRequestFailed,
				"GitHub could not answer the request.",
			)),
		}
	}

	fn url(&self, segments: &[&str]) -> Url {
		let mut url = self.api_base.clone();

		// `path_segments_mut` percent-encodes each segment, so a name with a
		// slash or dots cannot change the path.
		url.path_segments_mut()
			.expect("an https URL has a path")
			.extend(segments);

		url
	}

	/// Pagination links must stay on the API origin.
	fn same_api(&self, url: &Url) -> bool {
		url.scheme() == self.api_base.scheme()
			&& url.host_str() == self.api_base.host_str()
			&& url.port_or_known_default() == self.api_base.port_or_known_default()
	}
}

fn is_rate_limited(response: &HttpResponse) -> bool {
	response.status == 429
		|| response.rate_limit_remaining == Some(0)
		|| response.retry_after_seconds.is_some()
}

fn parse<D: for<'de> Deserialize<'de>>(response: &HttpResponse) -> Result<D, LauncherError> {
	serde_json::from_slice(&response.body).map_err(|error| {
		LauncherError::new(
			ErrorCode::ProviderResponseMalformed,
			"GitHub sent a response the launcher could not read.",
		)
		.with_source(error)
	})
}

#[derive(Deserialize)]
struct RawRepository {
	id: u64,
	name: String,
	#[serde(default)]
	private: bool,
	owner: RawOwner,
}

#[derive(Deserialize)]
struct RawOwner {
	id: u64,
	login: String,
}

#[derive(Deserialize)]
struct RawRelease {
	id: u64,
	tag_name: String,
	name: Option<String>,
	#[serde(default)]
	draft: bool,
	#[serde(default)]
	prerelease: bool,
	published_at: Option<String>,
	body: Option<String>,
	html_url: String,
	#[serde(default)]
	assets: Vec<RawAsset>,
}

#[derive(Deserialize)]
struct RawAsset {
	id: u64,
	name: String,
	#[serde(default)]
	size: u64,
	#[serde(default)]
	content_type: String,
	#[serde(default)]
	state: String,
	digest: Option<String>,
	browser_download_url: String,
}

impl From<RawRelease> for Release {
	fn from(raw: RawRelease) -> Self {
		Self {
			id: raw.id,
			tag: raw.tag_name,
			name: raw.name,
			prerelease: raw.prerelease,
			published_at: raw.published_at,
			notes: raw
				.body
				.map(|body| body.chars().take(MAX_NOTES_CHARS).collect()),
			page_url: raw.html_url,
			assets: raw
				.assets
				.into_iter()
				// Assets that are still uploading are not downloadable yet.
				.filter(|asset| asset.state.is_empty() || asset.state == "uploaded")
				.map(Asset::from)
				.collect(),
		}
	}
}

impl From<RawAsset> for Asset {
	fn from(raw: RawAsset) -> Self {
		Self {
			id: raw.id,
			name: raw.name,
			size: raw.size,
			content_type: raw.content_type,
			host_sha256: raw
				.digest
				.and_then(|digest| digest.strip_prefix("sha256:").map(str::to_lowercase))
				.filter(|hex| hex.len() == 64 && hex.bytes().all(|b| b.is_ascii_hexdigit())),
			download_url: raw.browser_download_url,
		}
	}
}

#[cfg(test)]
mod tests {
	use std::collections::VecDeque;
	use std::sync::Mutex;

	use super::*;

	/// Replays canned responses and records the URLs asked for.
	struct Fake {
		responses: Mutex<VecDeque<HttpResponse>>,
		requested: Mutex<Vec<String>>,
	}

	impl Fake {
		fn new(responses: Vec<HttpResponse>) -> Self {
			Self {
				responses: Mutex::new(responses.into()),
				requested: Mutex::new(Vec::new()),
			}
		}
	}

	impl Transport for &Fake {
		async fn get(
			&self,
			url: Url,
			_: MetadataRequest,
			_: usize,
		) -> Result<HttpResponse, LauncherError> {
			self.requested.lock().unwrap().push(url.to_string());

			Ok(self
				.responses
				.lock()
				.unwrap()
				.pop_front()
				.expect("an unexpected request"))
		}
	}

	fn ok(body: &str) -> HttpResponse {
		HttpResponse {
			status: 200,
			etag: None,
			retry_after_seconds: None,
			rate_limit_remaining: None,
			rate_limit_reset: None,
			next_page: None,
			body: body.as_bytes().to_vec(),
		}
	}

	fn status(code: u16) -> HttpResponse {
		HttpResponse {
			status: code,
			..ok("{}")
		}
	}

	const REPO: &str = r#"{"id":100001,"name":"fixture-editor","private":false,"owner":{"id":100002,"login":"fixture-owner"}}"#;

	fn release(id: u64, tag: &str) -> String {
		format!(
			r#"{{"id":{id},"tag_name":"{tag}","name":null,"draft":false,"prerelease":false,
			"published_at":"2030-01-01T00:00:00Z","body":"notes","html_url":"https://github.com/o/r/releases/tag/{tag}",
			"assets":[
				{{"id":{id}1,"name":"app-linux-x64.tar.gz","size":10,"content_type":"application/gzip","state":"uploaded",
				  "digest":"sha256:{digest}","browser_download_url":"https://github.com/o/r/releases/download/{tag}/app-linux-x64.tar.gz"}},
				{{"id":{id}2,"name":"half.zip","size":1,"content_type":"","state":"starter","digest":null,
				  "browser_download_url":"https://github.com/x"}}
			]}}"#,
			digest = "AB".repeat(32)
		)
	}

	#[tokio::test]
	async fn reads_repository_identity() {
		let fake = Fake::new(vec![ok(REPO)]);
		let provider = GithubProvider::new(&fake);

		let identity = provider.repository_by_id(100001).await.unwrap();

		assert_eq!(identity.owner_id, 100002);
		assert_eq!(identity.display_name(), "fixture-owner/fixture-editor");
		assert_eq!(
			fake.requested.lock().unwrap()[0],
			"https://api.github.com/repositories/100001"
		);
	}

	#[tokio::test]
	async fn names_are_encoded_into_the_path() {
		let fake = Fake::new(vec![ok(REPO)]);
		let provider = GithubProvider::new(&fake);

		provider.repository_by_name("o", "a/b?c").await.unwrap();

		assert_eq!(
			fake.requested.lock().unwrap()[0],
			"https://api.github.com/repos/o/a%2Fb%3Fc"
		);
	}

	#[tokio::test]
	async fn parses_releases_and_normalizes_digests() {
		let page = format!("[{},{}]", release(2, "v2"), release(1, "v1"));
		let fake = Fake::new(vec![ok(&page)]);
		let identity = RepositoryIdentity {
			repository_id: 100001,
			owner_id: 100002,
			owner: "fixture-owner".into(),
			name: "fixture-editor".into(),
		};

		let observation = GithubProvider::new(&fake).releases(identity).await.unwrap();

		assert!(observation.complete);
		assert_eq!(observation.releases.len(), 2);

		let first = &observation.releases[0];
		assert_eq!(first.tag, "v2");
		assert_eq!(first.assets.len(), 1, "the half-uploaded asset is skipped");
		assert_eq!(
			first.assets[0].host_sha256.as_deref(),
			Some("ab".repeat(32).as_str())
		);
	}

	#[tokio::test]
	async fn drafts_are_ignored() {
		let draft = release(3, "v3").replace("\"draft\":false", "\"draft\":true");
		let fake = Fake::new(vec![ok(&format!("[{draft}]"))]);
		let identity = RepositoryIdentity {
			repository_id: 1,
			owner_id: 2,
			owner: "o".into(),
			name: "r".into(),
		};

		let observation = GithubProvider::new(&fake).releases(identity).await.unwrap();

		assert!(observation.releases.is_empty());
	}

	#[tokio::test]
	async fn paging_stops_at_the_limit_and_reports_incomplete() {
		let next = Url::parse("https://api.github.com/repositories/1/releases?page=2").unwrap();
		let page = |tag: &str| HttpResponse {
			next_page: Some(next.clone()),
			..ok(&format!("[{}]", release(1, tag)))
		};
		let fake = Fake::new(vec![page("a"), page("b"), page("c")]);
		let identity = RepositoryIdentity {
			repository_id: 1,
			owner_id: 2,
			owner: "o".into(),
			name: "r".into(),
		};

		let observation = GithubProvider::new(&fake).releases(identity).await.unwrap();

		assert!(!observation.complete);
		assert_eq!(observation.releases.len(), 3);
	}

	#[tokio::test]
	async fn pagination_links_to_other_hosts_are_not_followed() {
		let elsewhere = Url::parse("https://evil.example/releases?page=2").unwrap();
		let first = HttpResponse {
			next_page: Some(elsewhere),
			..ok(&format!("[{}]", release(1, "a")))
		};
		let fake = Fake::new(vec![first]);
		let identity = RepositoryIdentity {
			repository_id: 1,
			owner_id: 2,
			owner: "o".into(),
			name: "r".into(),
		};

		let observation = GithubProvider::new(&fake).releases(identity).await.unwrap();

		assert!(observation.complete);
		assert_eq!(fake.requested.lock().unwrap().len(), 1);
	}

	#[tokio::test]
	async fn statuses_map_to_distinct_errors() {
		let rate_limited = HttpResponse {
			rate_limit_remaining: Some(0),
			..status(403)
		};
		let cases = [
			(status(404), ErrorCode::ProviderNotFound),
			(rate_limited, ErrorCode::ProviderRateLimited),
			(status(429), ErrorCode::ProviderRateLimited),
			(status(403), ErrorCode::ProviderDenied),
			(status(401), ErrorCode::ProviderDenied),
			(status(500), ErrorCode::NetworkRequestFailed),
		];

		for (response, expected) in cases {
			let fake = Fake::new(vec![response]);
			let error = GithubProvider::new(&fake)
				.repository_by_id(1)
				.await
				.unwrap_err();

			assert_eq!(error.code(), expected);
		}
	}

	#[tokio::test]
	async fn malformed_json_is_reported_not_trusted() {
		let fake = Fake::new(vec![ok("<html>not json</html>")]);

		let error = GithubProvider::new(&fake)
			.repository_by_id(1)
			.await
			.unwrap_err();

		assert_eq!(error.code(), ErrorCode::ProviderResponseMalformed);
	}

	#[tokio::test]
	async fn private_repositories_are_refused() {
		let body = REPO.replace("\"private\":false", "\"private\":true");
		let fake = Fake::new(vec![ok(&body)]);

		let error = GithubProvider::new(&fake)
			.repository_by_id(1)
			.await
			.unwrap_err();

		assert_eq!(error.code(), ErrorCode::ProviderDenied);
	}
}
