//! A portable list of apps to share or move between computers.
//!
//! A collection holds references to consider, never code to run. It has no
//! paths, no credentials, no approvals, and no claims about who made an app.
//! Importing one only adds favorites; it never installs anything and never
//! makes a repository count as reviewed. The format is versioned and
//! strictly limited, and a file with any field this version does not know is
//! rejected whole, because an unknown field could be an instruction.

use launcher_model::{ErrorCode, LauncherError};
use serde::{Deserialize, Serialize};

use crate::engine::{Engine, now_unix};

pub const FORMAT: &str = "remade-collection";
pub const VERSION: u32 = 1;
/// Larger files are refused before they are parsed.
pub const MAX_BYTES: usize = 256 * 1024;
const MAX_ENTRIES: usize = 1_000;
const MAX_NOTE: usize = 200;

/// One app, by reference.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Entry {
	/// A name in the Remade catalog.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub catalog_name: Option<String>,
	/// An `owner/repository` on GitHub. Does not make it reviewed.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub repository: Option<String>,
	/// A short note for whoever reads the file. Display only.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub note: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Collection {
	pub format: String,
	pub version: u32,
	pub apps: Vec<Entry>,
}

/// Why a file was not accepted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CollectionProblem {
	TooLarge,
	NotJson(String),
	WrongFormat,
	NewerVersion(u32),
	TooManyEntries,
	EmptyEntry(usize),
	TwoReferences(usize),
	BadCatalogName(usize),
	BadRepository(usize),
	LongNote(usize),
}

impl std::fmt::Display for CollectionProblem {
	fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			Self::TooLarge => write!(formatter, "That file is larger than a collection can be."),
			Self::NotJson(why) => write!(formatter, "That is not a collection file: {why}"),
			Self::WrongFormat => write!(formatter, "That file is not a Remade collection."),
			Self::NewerVersion(version) => write!(
				formatter,
				"That collection is version {version}, which this launcher does not understand."
			),
			Self::TooManyEntries => write!(
				formatter,
				"That collection has more apps than the limit of {MAX_ENTRIES}."
			),
			Self::EmptyEntry(number) => write!(
				formatter,
				"App {number} names neither a catalog name nor a repository."
			),
			Self::TwoReferences(number) => write!(
				formatter,
				"App {number} gives both a catalog name and a repository. Give one."
			),
			Self::BadCatalogName(number) => write!(
				formatter,
				"App {number} has a catalog name that is not valid."
			),
			Self::BadRepository(number) => write!(
				formatter,
				"App {number} has a repository that is not owner/repository."
			),
			Self::LongNote(number) => write!(
				formatter,
				"App {number} has a note longer than {MAX_NOTE} characters."
			),
		}
	}
}

fn is_catalog_name(text: &str) -> bool {
	(1..=64).contains(&text.len())
		&& text
			.chars()
			.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

fn is_repository(text: &str) -> bool {
	let part = |part: &str| {
		(1..=100).contains(&part.len())
			&& part
				.chars()
				.all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
			&& part != "."
			&& part != ".."
	};

	matches!(text.split_once('/'), Some((owner, name)) if part(owner) && part(name))
}

impl Collection {
	/// Reads and checks a file's bytes. A file with anything unexpected in it
	/// is refused whole.
	pub fn parse(bytes: &[u8]) -> Result<Self, CollectionProblem> {
		if bytes.len() > MAX_BYTES {
			return Err(CollectionProblem::TooLarge);
		}

		let collection: Self = serde_json::from_slice(bytes)
			.map_err(|error| CollectionProblem::NotJson(error.to_string()))?;

		collection.check()?;

		Ok(collection)
	}

	fn check(&self) -> Result<(), CollectionProblem> {
		if self.format != FORMAT {
			return Err(CollectionProblem::WrongFormat);
		}

		if self.version > VERSION {
			return Err(CollectionProblem::NewerVersion(self.version));
		}

		if self.apps.len() > MAX_ENTRIES {
			return Err(CollectionProblem::TooManyEntries);
		}

		for (index, entry) in self.apps.iter().enumerate() {
			let number = index + 1;

			match (&entry.catalog_name, &entry.repository) {
				(None, None) => return Err(CollectionProblem::EmptyEntry(number)),
				(Some(_), Some(_)) => return Err(CollectionProblem::TwoReferences(number)),
				(Some(name), None) if !is_catalog_name(name) => {
					return Err(CollectionProblem::BadCatalogName(number));
				}
				(None, Some(repository)) if !is_repository(repository) => {
					return Err(CollectionProblem::BadRepository(number));
				}
				_ => {}
			}

			if entry.note.as_ref().is_some_and(|note| {
				note.chars().count() > MAX_NOTE || note.chars().any(char::is_control)
			}) {
				return Err(CollectionProblem::LongNote(number));
			}
		}

		Ok(())
	}

	pub fn to_json(&self) -> String {
		serde_json::to_string_pretty(self).unwrap_or_default()
	}
}

/// What importing an entry would do.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EntryOutcome {
	/// A catalog name that is in the catalog.
	InCatalog { name: String },
	/// A catalog name the catalog does not list, or no catalog to check.
	NotInCatalog,
	/// A GitHub repository. It will not count as reviewed.
	Repository,
	/// Already installed, so there is nothing to add.
	AlreadyInstalled,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EntryPreview {
	pub entry: Entry,
	pub outcome: EntryOutcome,
}

fn favorite_of(entry: &Entry) -> (&'static str, String) {
	match (&entry.catalog_name, &entry.repository) {
		(Some(name), _) => ("catalog", name.clone()),
		(_, Some(repository)) => ("repository", repository.clone()),
		(None, None) => ("catalog", String::new()),
	}
}

impl Engine {
	/// What is installed and what is marked as a favorite, as a collection.
	/// Nothing private goes in: no folders, no versions, no approvals.
	pub async fn export_collection(&self) -> Result<Collection, LauncherError> {
		let mut apps: Vec<Entry> = Vec::new();
		let mut seen = std::collections::HashSet::new();
		let mut add = |entry: Entry| {
			if seen.insert(favorite_of(&entry)) {
				apps.push(entry);
			}
		};

		for app in self.installed().await? {
			match (&app.catalog_slug, app.curated) {
				(Some(slug), true) => add(Entry {
					catalog_name: Some(slug.clone()),
					repository: None,
					note: None,
				}),
				_ if is_repository(&app.repository) => add(Entry {
					catalog_name: None,
					repository: Some(app.repository.clone()),
					note: None,
				}),
				_ => {}
			}
		}

		for favorite in self.database.favorites().await? {
			add(match favorite.kind.as_str() {
				"catalog" => Entry {
					catalog_name: Some(favorite.reference),
					repository: None,
					note: None,
				},
				_ => Entry {
					catalog_name: None,
					repository: Some(favorite.reference),
					note: None,
				},
			});
		}

		Ok(Collection {
			format: FORMAT.into(),
			version: VERSION,
			apps,
		})
	}

	/// Says what importing would do, without doing it.
	pub async fn preview_collection(
		&self,
		collection: &Collection,
	) -> Result<Vec<EntryPreview>, LauncherError> {
		let installed = self.installed().await?;
		let state = self.catalog().state();

		Ok(collection
			.apps
			.iter()
			.map(|entry| {
				let outcome = match (&entry.catalog_name, &entry.repository) {
					(Some(name), _) => {
						if installed
							.iter()
							.any(|app| app.catalog_slug.as_deref() == Some(name.as_str()))
						{
							EntryOutcome::AlreadyInstalled
						} else {
							match state
								.catalog
								.as_ref()
								.and_then(|catalog| catalog.project(name))
							{
								Some(project) => EntryOutcome::InCatalog {
									name: project.display.name.clone(),
								},
								None => EntryOutcome::NotInCatalog,
							}
						}
					}
					(None, Some(repository)) => {
						if installed
							.iter()
							.any(|app| app.repository.eq_ignore_ascii_case(repository))
						{
							EntryOutcome::AlreadyInstalled
						} else {
							EntryOutcome::Repository
						}
					}
					(None, None) => EntryOutcome::NotInCatalog,
				};

				EntryPreview {
					entry: entry.clone(),
					outcome,
				}
			})
			.collect())
	}

	/// Adds the collection's entries as favorites. Installs nothing. Returns
	/// how many were new.
	pub async fn import_collection(&self, collection: &Collection) -> Result<usize, LauncherError> {
		collection.check().map_err(|problem| {
			LauncherError::new(ErrorCode::InvalidSourceUrl, problem.to_string())
		})?;

		let now = now_unix();
		let mut added = 0;

		for entry in &collection.apps {
			let (kind, reference) = favorite_of(entry);

			if self.database.add_favorite(kind, reference, now).await? {
				added += 1;
			}
		}

		Ok(added)
	}

	pub async fn favorites(&self) -> Result<Vec<Entry>, LauncherError> {
		Ok(self
			.database
			.favorites()
			.await?
			.into_iter()
			.map(|favorite| match favorite.kind.as_str() {
				"catalog" => Entry {
					catalog_name: Some(favorite.reference),
					repository: None,
					note: None,
				},
				_ => Entry {
					catalog_name: None,
					repository: Some(favorite.reference),
					note: None,
				},
			})
			.collect())
	}

	/// Removes a favorite by its catalog name or `owner/repository`.
	pub async fn remove_favorite(&self, reference: &str) -> Result<bool, LauncherError> {
		let kind = if reference.contains('/') {
			"repository"
		} else {
			"catalog"
		};

		self.database
			.remove_favorite(kind, reference.to_owned())
			.await
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::DataDirectory;

	fn json(apps: &str) -> Vec<u8> {
		format!(r#"{{"format":"remade-collection","version":1,"apps":{apps}}}"#).into_bytes()
	}

	#[test]
	fn a_valid_collection_is_read() {
		let parsed = Collection::parse(&json(
			r#"[{"catalog_name":"quill"},{"repository":"owner/repo","note":"nice"}]"#,
		))
		.unwrap();

		assert_eq!(parsed.apps.len(), 2);
		assert_eq!(
			Collection::parse(parsed.to_json().as_bytes()).unwrap(),
			parsed
		);
	}

	#[test]
	fn anything_unexpected_rejects_the_whole_file() {
		for hostile in [
			r#"[{"catalog_name":"quill","run":"rm -rf ~"}]"#,
			r#"[{"catalog_name":"quill","install_command":"curl x | sh"}]"#,
			r#"[{"catalog_name":"Quill"}]"#,
			r#"[{"catalog_name":"../quill"}]"#,
			r#"[{"repository":"not-a-repo"}]"#,
			r#"[{"repository":"a/b/c"}]"#,
			r#"[{"repository":"../etc"}]"#,
			r#"[{"catalog_name":"a","repository":"o/r"}]"#,
			r#"[{}]"#,
			r#"[{"catalog_name":"a","note":"line\nbreak"}]"#,
		] {
			assert!(Collection::parse(&json(hostile)).is_err(), "{hostile}");
		}

		let extra = br#"{"format":"remade-collection","version":1,"apps":[],"trusted_keys":["x"]}"#;

		assert!(
			Collection::parse(extra).is_err(),
			"an unknown top-level field"
		);
	}

	#[test]
	fn the_wrong_format_a_newer_version_and_big_files_are_refused() {
		assert_eq!(
			Collection::parse(br#"{"format":"other","version":1,"apps":[]}"#),
			Err(CollectionProblem::WrongFormat)
		);
		assert_eq!(
			Collection::parse(br#"{"format":"remade-collection","version":2,"apps":[]}"#),
			Err(CollectionProblem::NewerVersion(2))
		);
		assert_eq!(
			Collection::parse(&vec![b' '; MAX_BYTES + 1]),
			Err(CollectionProblem::TooLarge)
		);
		assert!(matches!(
			Collection::parse(b"not json"),
			Err(CollectionProblem::NotJson(_))
		));

		let many = format!(
			"[{}]",
			vec![r#"{"catalog_name":"a"}"#; MAX_ENTRIES + 1].join(",")
		);

		assert_eq!(
			Collection::parse(&json(&many)),
			Err(CollectionProblem::TooManyEntries)
		);
	}

	async fn engine() -> (tempfile::TempDir, Engine) {
		let directory = tempfile::tempdir().unwrap();
		let engine = Engine::open(&DataDirectory::at(directory.path().join("data")))
			.await
			.unwrap();

		(directory, engine)
	}

	#[tokio::test]
	async fn importing_adds_favorites_only_and_exporting_carries_nothing_private() {
		let (_directory, engine) = engine().await;
		let collection = Collection::parse(&json(
			r#"[{"catalog_name":"quill"},{"repository":"owner/repo"}]"#,
		))
		.unwrap();

		let preview = engine.preview_collection(&collection).await.unwrap();

		assert_eq!(
			preview[0].outcome,
			EntryOutcome::NotInCatalog,
			"no catalog is loaded to confirm it"
		);
		assert_eq!(preview[1].outcome, EntryOutcome::Repository);
		assert!(
			engine.favorites().await.unwrap().is_empty(),
			"previewing changes nothing"
		);

		assert_eq!(engine.import_collection(&collection).await.unwrap(), 2);
		assert_eq!(
			engine.import_collection(&collection).await.unwrap(),
			0,
			"a second import adds nothing"
		);
		assert!(
			engine.installed().await.unwrap().is_empty(),
			"nothing was installed"
		);

		let exported = engine.export_collection().await.unwrap();

		assert_eq!(exported.apps.len(), 2);

		let text = exported.to_json();

		assert!(
			!text.contains(engine.data_directory().root().to_str().unwrap()),
			"no folders in an export"
		);
		assert!(engine.remove_favorite("quill").await.unwrap());
		assert!(engine.remove_favorite("owner/repo").await.unwrap());
		assert!(engine.favorites().await.unwrap().is_empty());
	}
}

/// The largest collection file, for callers that read the file themselves.
pub const fn collection_max_bytes() -> usize {
	MAX_BYTES
}
