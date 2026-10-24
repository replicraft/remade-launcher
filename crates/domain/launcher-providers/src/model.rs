use serde::Serialize;

/// Who owns a repository, by stable numeric ID. Names can change; IDs do not.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct RepositoryIdentity {
	pub repository_id: u64,
	pub owner_id: u64,
	pub owner: String,
	pub name: String,
}

impl RepositoryIdentity {
	/// `owner/name`, as the provider reports it today.
	pub fn display_name(&self) -> String {
		format!("{}/{}", self.owner, self.name)
	}
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Asset {
	pub id: u64,
	pub name: String,
	pub size: u64,
	pub content_type: String,
	/// SHA-256 reported by the host, lowercase hex. This is the host's claim,
	/// not publisher evidence.
	pub host_sha256: Option<String>,
	pub download_url: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Release {
	pub id: u64,
	pub tag: String,
	pub name: Option<String>,
	pub prerelease: bool,
	pub published_at: Option<String>,
	pub notes: Option<String>,
	pub page_url: String,
	pub assets: Vec<Asset>,
}

/// What the provider said at one moment.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ReleaseObservation {
	pub repository: RepositoryIdentity,
	pub releases: Vec<Release>,
	/// False when paging stopped at a limit. "Nothing compatible found" is
	/// then a statement about the checked releases only.
	pub complete: bool,
}
