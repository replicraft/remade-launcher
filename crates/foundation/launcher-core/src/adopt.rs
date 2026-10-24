//! Keeping an eye on an app that was already on the computer.
//!
//! The launcher did not put these files there, so it will not remove or
//! replace them. Adopting only records where the app is and how to start it,
//! so it shows up beside the apps the launcher manages. A name the person
//! gives it is a claim, never proof: nothing is updated or deleted because of
//! it. Dropping the record is always safe and never touches a file.

use std::path::{Path, PathBuf};

use launcher_model::{ErrorCode, InstallationId, LauncherError};
use launcher_storage::AdoptedRecord;

use crate::engine::{Engine, now_unix};
use crate::lifecycle::spawn_app;
use crate::saved_plans::Approval;

/// How many entries are counted before the scan stops.
const SCAN_LIMIT: usize = 200_000;

fn refused(why: impl Into<String>) -> LauncherError {
	LauncherError::new(ErrorCode::AdoptionRefused, why.into())
}

/// What looking at a folder found. Nothing has been recorded.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AdoptionCandidate {
	pub path: PathBuf,
	pub suggested_name: String,
	/// Things that could start the app, relative to the folder.
	pub entry_points: Vec<String>,
	pub files: usize,
	pub bytes: u64,
	/// The scan stopped early, so the counts are lower bounds.
	pub truncated: bool,
}

/// What the person asks for.
#[derive(Clone, Debug)]
pub struct AdoptionRequest {
	pub path: PathBuf,
	pub name: Option<String>,
	/// The catalog entry the person says this is. Optional, and only a claim.
	pub catalog_slug: Option<String>,
	pub entry_point: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AdoptedApp {
	pub id: InstallationId,
	pub name: String,
	pub path: PathBuf,
	pub entry_point: Option<String>,
	pub catalog_slug: Option<String>,
	pub adopted_at: i64,
	pub last_seen_at: Option<i64>,
	/// The folder is there now. A drive that is unplugged is unavailable, not
	/// removed.
	pub present: bool,
}

impl AdoptedApp {
	fn from_record(record: AdoptedRecord) -> Self {
		let path = PathBuf::from(&record.path);

		Self {
			id: record.id,
			name: record.display_name,
			present: path.is_dir(),
			path,
			entry_point: record.entry_point,
			catalog_slug: record.catalog_slug,
			adopted_at: record.adopted_at,
			last_seen_at: record.last_seen_at,
		}
	}
}

/// Why a folder is too broad or too important to watch, if it is.
///
/// This is about the path alone. A folder like a home directory or a system
/// location holds far more than one app, and "adopting" it would mislead.
pub(crate) fn too_broad(path: &Path, home: Option<&Path>) -> Option<&'static str> {
	use std::path::Component;

	let names: Vec<String> = path
		.components()
		.filter_map(|component| match component {
			Component::Normal(name) => Some(name.to_string_lossy().to_lowercase()),
			_ => None,
		})
		.collect();

	if names.len() < 2 {
		return Some("That is a top-level folder, which holds far more than one app.");
	}

	if let Some(home) = home {
		if path == home {
			return Some("That is your home folder, not an app.");
		}

		const PERSONAL: [&str; 8] = [
			"documents",
			"desktop",
			"downloads",
			"pictures",
			"music",
			"movies",
			"videos",
			"library",
		];

		if path.parent() == Some(home)
			&& names
				.last()
				.is_some_and(|name| PERSONAL.contains(&name.as_str()))
		{
			return Some("That is one of your personal folders, not an app.");
		}
	}

	let first = names[0].as_str();

	if matches!(
		first,
		"system"
			| "bin" | "sbin"
			| "etc" | "var"
			| "dev" | "proc"
			| "sys" | "boot"
			| "lib" | "lib64"
	) || (first == "usr" && names.get(1).is_some_and(|second| second != "local"))
	{
		return Some("That is part of the operating system.");
	}

	// The shared folders programs are installed into, but not what is in them.
	let shared = matches!(
		names.as_slice(),
		[a, b] if (a == "usr" && b == "local") || (a == "users" && b == "shared")
	) || matches!(
		names.last().map(String::as_str),
		Some("program files" | "program files (x86)" | "windows" | "programdata" | "applications")
	) && names.len() <= 2;

	if shared {
		return Some("That folder holds many apps. Choose the one app's own folder.");
	}

	None
}

/// Candidate start files: a `.app` bundle or an executable at the top of the
/// folder or one level down.
fn find_entry_points(root: &Path) -> Vec<String> {
	let mut found = Vec::new();
	let Ok(entries) = std::fs::read_dir(root) else {
		return found;
	};

	for entry in entries.flatten() {
		let name = entry.file_name().to_string_lossy().into_owned();
		let Ok(kind) = entry.file_type() else {
			continue;
		};

		let starts_the_app = (kind.is_dir() && name.ends_with(".app"))
			|| (kind.is_file() && is_executable(&entry.path()));

		if starts_the_app {
			found.push(name);
		} else if kind.is_dir()
			&& let Ok(inner) = std::fs::read_dir(entry.path())
		{
			for child in inner.flatten() {
				let child_name = child.file_name().to_string_lossy().into_owned();

				if child.file_type().is_ok_and(|kind| kind.is_file())
					&& is_executable(&child.path())
				{
					found.push(format!("{name}/{child_name}"));
				}
			}
		}
	}

	found.sort();
	found.dedup();
	found.truncate(20);

	found
}

#[cfg(unix)]
fn is_executable(path: &Path) -> bool {
	use std::os::unix::fs::PermissionsExt;

	std::fs::metadata(path)
		.is_ok_and(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
}

#[cfg(not(unix))]
fn is_executable(path: &Path) -> bool {
	path.extension()
		.is_some_and(|ext| ext.eq_ignore_ascii_case("exe"))
}

/// Counts what is under `root` without following links, stopping at the limit.
fn scan(root: &Path) -> (usize, u64, bool) {
	let mut files = 0;
	let mut bytes = 0;
	let mut pending = vec![root.to_path_buf()];

	while let Some(folder) = pending.pop() {
		let Ok(entries) = std::fs::read_dir(&folder) else {
			continue;
		};

		for entry in entries.flatten() {
			files += 1;

			if files >= SCAN_LIMIT {
				return (files, bytes, true);
			}

			match entry.file_type() {
				Ok(kind) if kind.is_dir() => pending.push(entry.path()),
				Ok(kind) if kind.is_file() => {
					bytes += entry.metadata().map_or(0, |metadata| metadata.len())
				}
				_ => {}
			}
		}
	}

	(files, bytes, false)
}

fn suggested_name(path: &Path) -> String {
	let name = path
		.file_name()
		.map(|name| name.to_string_lossy().into_owned())
		.unwrap_or_default();

	name.strip_suffix(".app")
		.map_or(name.clone(), str::to_owned)
}

impl Engine {
	/// Looks at a folder and says whether it could be adopted. Changes
	/// nothing.
	pub async fn inspect_for_adoption(
		&self,
		path: &Path,
	) -> Result<AdoptionCandidate, LauncherError> {
		let path = path.to_path_buf();
		let data = self.data.root().to_path_buf();
		let managed: Vec<PathBuf> = self
			.database
			.installations()
			.await?
			.into_iter()
			.map(|record| PathBuf::from(record.root))
			.collect();
		let adopted: Vec<PathBuf> = self
			.database
			.adopted_apps()
			.await?
			.into_iter()
			.map(|record| PathBuf::from(record.path))
			.collect();
		let home = directories::BaseDirs::new().map(|dirs| dirs.home_dir().to_path_buf());

		tokio::task::spawn_blocking(move || {
			inspect_blocking(&path, &data, &managed, &adopted, home.as_deref())
		})
		.await
		.map_err(|error| refused("The folder could not be examined.").with_source(error))?
	}

	/// Records an app the launcher did not install. The person must have
	/// approved it.
	pub async fn adopt(
		&self,
		request: AdoptionRequest,
		approval: &Approval,
	) -> Result<AdoptedApp, LauncherError> {
		if !approval.confirmed {
			return Err(LauncherError::new(
				ErrorCode::ConfirmationRequired,
				"This needs your confirmation before it runs.",
			));
		}

		let candidate = self.inspect_for_adoption(&request.path).await?;

		let entry_point = match request.entry_point {
			Some(entry) => {
				if !launcher_packages::is_safe_relative_path(&entry)
					|| !candidate.path.join(&entry).exists()
				{
					return Err(refused("That start file is not inside the folder."));
				}

				Some(entry)
			}
			None => match candidate.entry_points.as_slice() {
				[only] => Some(only.clone()),
				_ => None,
			},
		};

		if let Some(slug) = &request.catalog_slug
			&& self
				.catalog()
				.state()
				.catalog
				.as_ref()
				.is_some_and(|catalog| catalog.project(slug).is_none())
		{
			return Err(refused("That name is not in the catalog."));
		}

		let name = request
			.name
			.map(|name| name.trim().to_owned())
			.filter(|name| !name.is_empty())
			.unwrap_or(candidate.suggested_name);
		let now = now_unix();

		let record = AdoptedRecord {
			id: InstallationId::new(),
			display_name: name,
			path: candidate.path.display().to_string(),
			entry_point,
			catalog_slug: request.catalog_slug,
			adopted_at: now,
			last_seen_at: Some(now),
		};

		self.database
			.add_adopted(record.clone())
			.await
			.map_err(|error| refused("That folder is already being watched.").with_source(error))?;

		Ok(AdoptedApp::from_record(record))
	}

	pub async fn adopted_apps(&self) -> Result<Vec<AdoptedApp>, LauncherError> {
		Ok(self
			.database
			.adopted_apps()
			.await?
			.into_iter()
			.map(AdoptedApp::from_record)
			.collect())
	}

	/// Stops watching an app. Its files are not touched.
	pub async fn forget_adopted(&self, id: InstallationId) -> Result<bool, LauncherError> {
		self.database.remove_adopted(id).await
	}

	/// Starts an adopted app by its recorded start file, after checking that
	/// the file is still inside the folder.
	pub async fn launch_adopted(&self, id: InstallationId) -> Result<(), LauncherError> {
		let record = self.database.adopted_app(id).await?.ok_or_else(|| {
			LauncherError::new(
				ErrorCode::InstallationNotFound,
				"That app is not being watched.",
			)
		})?;

		let launched = tokio::task::spawn_blocking(move || -> Result<(), LauncherError> {
			let launch_failed =
				|| LauncherError::new(ErrorCode::LaunchFailed, "The app could not be started.");
			let root = PathBuf::from(&record.path)
				.canonicalize()
				.map_err(|_| launch_failed())?;
			let entry = record.entry_point.as_deref().ok_or_else(|| {
				LauncherError::new(
					ErrorCode::LaunchFailed,
					"The launcher does not know which file starts this app.",
				)
			})?;

			if !launcher_packages::is_safe_relative_path(entry) {
				return Err(launch_failed());
			}

			let target = root
				.join(entry)
				.canonicalize()
				.map_err(|_| launch_failed())?;

			// A link at the start file must not lead out of the folder.
			if !target.starts_with(&root) {
				return Err(launch_failed());
			}

			spawn_app(target, &root)
		})
		.await
		.map_err(|error| {
			LauncherError::new(
				ErrorCode::InternalInvariant,
				"The launcher hit an unexpected problem.",
			)
			.with_source(error)
		})?;

		if launched.is_ok() {
			let _ = self.database.mark_adopted_seen(id, now_unix()).await;
		}

		launched
	}
}

fn inspect_blocking(
	path: &Path,
	data: &Path,
	managed: &[PathBuf],
	adopted: &[PathBuf],
	home: Option<&Path>,
) -> Result<AdoptionCandidate, LauncherError> {
	let metadata =
		std::fs::symlink_metadata(path).map_err(|_| refused("That folder does not exist."))?;

	if metadata.file_type().is_symlink() {
		return Err(refused("That is a link. Give the folder it points to."));
	}

	if !metadata.is_dir() {
		return Err(refused("That is a file. Give the app's folder."));
	}

	let path = path
		.canonicalize()
		.map_err(|_| refused("That folder could not be read."))?;

	if let Some(why) = too_broad(&path, home) {
		return Err(refused(why));
	}

	// Compare real locations, so a path through a link or a differently
	// spelled temporary folder cannot slip past.
	let real = |other: &Path| other.canonicalize().unwrap_or_else(|_| other.to_path_buf());
	let overlaps = |other: &Path| {
		let other = real(other);

		path.starts_with(&other) || other.starts_with(&path)
	};

	if overlaps(data) {
		return Err(refused("That folder holds the launcher's own data."));
	}

	if managed.iter().any(|root| overlaps(root)) {
		return Err(refused(
			"The launcher already manages an app in that folder.",
		));
	}

	if adopted.iter().any(|root| overlaps(root)) {
		return Err(refused("That folder is already being watched."));
	}

	let (files, bytes, truncated) = scan(&path);

	Ok(AdoptionCandidate {
		suggested_name: suggested_name(&path),
		entry_points: find_entry_points(&path),
		files,
		bytes,
		truncated,
		path,
	})
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::DataDirectory;

	#[test]
	fn broad_and_system_folders_are_refused_by_path_alone() {
		let home = Path::new("/home/me");

		for broad in [
			"/",
			"/usr",
			"/usr/lib",
			"/System/Library",
			"/etc/thing",
			"/Applications",
			"/home/me",
			"/home/me/Documents",
			"/usr/local",
			"/opt",
		] {
			assert!(too_broad(Path::new(broad), Some(home)).is_some(), "{broad}");
		}

		for fine in [
			"/Applications/Quill.app",
			"/opt/quill",
			"/home/me/apps/quill",
			"/usr/local/quill",
			"/mnt/tools/quill",
		] {
			assert_eq!(too_broad(Path::new(fine), Some(home)), None, "{fine}");
		}
	}

	fn app_folder(root: &Path) -> PathBuf {
		let folder = root.join("tools/quill");

		std::fs::create_dir_all(folder.join("bin")).unwrap();
		std::fs::write(folder.join("README"), "hello").unwrap();
		write_executable(&folder.join("bin/quill"));

		folder
	}

	fn write_executable(path: &Path) {
		std::fs::write(path, "#!/bin/sh\nexit 0\n").unwrap();

		#[cfg(unix)]
		{
			use std::os::unix::fs::PermissionsExt;

			std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
		}
	}

	async fn engine() -> (tempfile::TempDir, Engine) {
		let directory = tempfile::tempdir().unwrap();
		let engine = Engine::open(&DataDirectory::at(directory.path().join("data")))
			.await
			.unwrap();

		(directory, engine)
	}

	fn approved() -> Approval {
		Approval {
			actor: "test".into(),
			confirmed: true,
			accepted_unreviewed_source: None,
		}
	}

	#[cfg(unix)]
	#[tokio::test]
	async fn looking_changes_nothing_and_finds_the_start_file() {
		let (directory, engine) = engine().await;
		let folder = app_folder(directory.path());

		let candidate = engine.inspect_for_adoption(&folder).await.unwrap();

		assert_eq!(candidate.suggested_name, "quill");
		assert_eq!(candidate.entry_points, vec!["bin/quill".to_owned()]);
		assert_eq!(candidate.files, 3);
		assert!(engine.adopted_apps().await.unwrap().is_empty());
	}

	#[cfg(unix)]
	#[tokio::test]
	async fn adopting_needs_approval_and_records_only_a_watch() {
		let (directory, engine) = engine().await;
		let folder = app_folder(directory.path());
		let request = AdoptionRequest {
			path: folder.clone(),
			name: Some("Quill".into()),
			catalog_slug: None,
			entry_point: None,
		};

		let declined = Approval {
			confirmed: false,
			..approved()
		};

		assert_eq!(
			engine
				.adopt(request.clone(), &declined)
				.await
				.unwrap_err()
				.code(),
			ErrorCode::ConfirmationRequired
		);

		let adopted = engine.adopt(request.clone(), &approved()).await.unwrap();

		assert_eq!(adopted.name, "Quill");
		assert_eq!(adopted.entry_point.as_deref(), Some("bin/quill"));
		assert!(adopted.present);

		// The folder is untouched, and a second adoption of it is refused.
		assert!(folder.join("README").exists());
		assert_eq!(
			engine.adopt(request, &approved()).await.unwrap_err().code(),
			ErrorCode::AdoptionRefused
		);
		assert!(
			engine.installed().await.unwrap().is_empty(),
			"an adopted app is not an install"
		);
	}

	#[tokio::test]
	async fn forgetting_drops_the_record_and_leaves_every_file() {
		let (directory, engine) = engine().await;
		let folder = app_folder(directory.path());
		let adopted = engine
			.adopt(
				AdoptionRequest {
					path: folder.clone(),
					name: None,
					catalog_slug: None,
					entry_point: Some("README".into()),
				},
				&approved(),
			)
			.await
			.unwrap();

		assert!(engine.forget_adopted(adopted.id).await.unwrap());
		assert!(!engine.forget_adopted(adopted.id).await.unwrap());
		assert!(folder.join("bin").exists() && folder.join("README").exists());
	}

	#[tokio::test]
	async fn the_launchers_own_data_and_links_and_files_are_refused() {
		let (directory, engine) = engine().await;
		let folder = app_folder(directory.path());

		assert!(
			engine
				.inspect_for_adoption(&directory.path().join("data"))
				.await
				.is_err(),
			"the launcher's own data"
		);
		assert!(
			engine
				.inspect_for_adoption(&folder.join("README"))
				.await
				.is_err(),
			"a file"
		);
		assert!(
			engine
				.inspect_for_adoption(&directory.path().join("missing"))
				.await
				.is_err()
		);

		#[cfg(unix)]
		{
			let link = directory.path().join("link");

			std::os::unix::fs::symlink(&folder, &link).unwrap();

			assert!(
				engine.inspect_for_adoption(&link).await.is_err(),
				"a symlink"
			);
		}
	}

	#[tokio::test]
	async fn a_start_file_outside_the_folder_is_refused() {
		let (directory, engine) = engine().await;
		let folder = app_folder(directory.path());

		for entry in ["../elsewhere", "/etc/passwd", "missing"] {
			let request = AdoptionRequest {
				path: folder.clone(),
				name: None,
				catalog_slug: None,
				entry_point: Some(entry.into()),
			};

			assert_eq!(
				engine.adopt(request, &approved()).await.unwrap_err().code(),
				ErrorCode::AdoptionRefused,
				"{entry}"
			);
		}
	}

	#[cfg(unix)]
	#[tokio::test]
	async fn launching_runs_the_recorded_start_file_from_inside_the_folder() {
		let (directory, engine) = engine().await;
		let folder = directory.path().join("tools/runner");

		std::fs::create_dir_all(&folder).unwrap();
		std::fs::write(folder.join("go"), "#!/bin/sh\necho ran > ran.txt\n").unwrap();

		{
			use std::os::unix::fs::PermissionsExt;

			std::fs::set_permissions(folder.join("go"), std::fs::Permissions::from_mode(0o755))
				.unwrap();
		}

		let adopted = engine
			.adopt(
				AdoptionRequest {
					path: folder.clone(),
					name: None,
					catalog_slug: None,
					entry_point: Some("go".into()),
				},
				&approved(),
			)
			.await
			.unwrap();

		engine.launch_adopted(adopted.id).await.unwrap();

		let marker = folder.join("ran.txt");

		for _ in 0..100 {
			if marker.exists() {
				break;
			}

			tokio::time::sleep(std::time::Duration::from_millis(50)).await;
		}

		assert!(marker.exists(), "the start file ran");
	}

	#[tokio::test]
	async fn an_unplugged_folder_is_unavailable_not_gone() {
		let (directory, engine) = engine().await;
		let folder = app_folder(directory.path());

		engine
			.adopt(
				AdoptionRequest {
					path: folder.clone(),
					name: None,
					catalog_slug: None,
					entry_point: Some("README".into()),
				},
				&approved(),
			)
			.await
			.unwrap();
		std::fs::remove_dir_all(&folder).unwrap();

		let apps = engine.adopted_apps().await.unwrap();

		assert_eq!(apps.len(), 1, "the record stays");
		assert!(!apps[0].present);
	}
}
