use launcher_model::InstallationId;
use launcher_storage::{InstallationRecord, OperationRecord};
use serde::{Deserialize, Serialize};

/// Presentation facts the plan does not carry.
#[derive(Clone, Debug)]
pub struct InstallMeta {
	pub display_name: String,
	/// The catalog slug, for curated installs.
	pub catalog_slug: Option<String>,
}

/// One owned file, as recorded in a receipt.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct ManifestEntry {
	pub path: String,
	pub kind: String,
	pub size: u64,
	pub sha256: String,
}

/// An installed app, as the interface and CLI see it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InstalledApp {
	pub id: InstallationId,
	pub name: String,
	pub repository: String,
	pub version: String,
	pub curated: bool,
	pub catalog_slug: Option<String>,
	pub root: String,
	pub launchable: bool,
	pub evidence: String,
	pub installed_at: i64,
}

impl From<InstallationRecord> for InstalledApp {
	fn from(record: InstallationRecord) -> Self {
		Self {
			id: record.id,
			name: record.display_name,
			repository: record.repository,
			version: record.release_tag,
			curated: record.origin == "curated",
			catalog_slug: record.catalog_slug,
			root: record.root,
			launchable: record.entry_point.is_some(),
			evidence: record.evidence,
			installed_at: record.installed_at,
		}
	}
}

/// One line of operation history.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OperationSummary {
	/// The operation's full ID.
	pub id: String,
	pub kind: String,
	pub state: String,
	pub app: Option<String>,
	pub detail: Option<String>,
	pub started_at: i64,
}

impl From<OperationRecord> for OperationSummary {
	fn from(record: OperationRecord) -> Self {
		// The plan is JSON written by the launcher, so reading the repository
		// back out of it is safe and gives history a human label.
		let app = serde_json::from_str::<serde_json::Value>(&record.plan)
			.ok()
			.and_then(|plan| plan["source"]["display"].as_str().map(str::to_owned));

		Self {
			id: record.id.to_string(),
			kind: record.kind,
			state: record.state,
			app,
			detail: record.detail,
			started_at: record.created_at,
		}
	}
}
