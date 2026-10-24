//! The launcher keeping itself up to date.
//!
//! This is separate from how installed apps are updated. The choice lives in
//! [`LauncherUpdates`](launcher_model::LauncherUpdates), and it never touches
//! the app update mode, the pins, or the consent records.
//!
//! The release document is signed with a key that is only good for launcher
//! releases. A new launcher is downloaded, checked against the size and
//! SHA-256 in that signed document, unpacked safely, and kept aside. It is
//! switched in later, under the operation lock, so it never replaces the
//! program in the middle of an install. The previous files are kept for one
//! step back.

mod layout;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use launcher_catalog::{
	CatalogError, HighWater, KeyRegistry, MAX_RELEASE_BYTES, ReleaseContext, VerifiedRelease,
	limits, verify_release,
};
use launcher_model::{ErrorCode, LauncherError, LauncherUpdates, product};
use launcher_net::Url;
use launcher_packages::{Limits, detect_magic, extract, inspect};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

pub use layout::InstallLayout;
use layout::{EXECUTABLES, Replacement, hash_file, replace_files};

use crate::engine::{Engine, now_unix};

const HIGH_WATER_KEY: &str = "launcher.release.high_water";
const CHECKED_KEY: &str = "launcher.update.checked_at";
const AVAILABLE_KEY: &str = "launcher.update.available";
const STAGED_KEY: &str = "launcher.update.staged";
const PREVIOUS_KEY: &str = "launcher.update.previous";
const APPLIED_KEY: &str = "launcher.update.applied";

/// How long an automatic check waits before looking again.
const CHECK_INTERVAL_SECONDS: i64 = 6 * 60 * 60;

/// The release document is tiny. These are generous ceilings.
const MAX_DOCUMENT_BYTES: usize = 256 * 1024;

/// How many times a mismatched document and signature are fetched again. A
/// CDN can briefly serve a new document beside an old signature.
const PAIR_ATTEMPTS: usize = 2;

/// Where launcher releases are announced and who may sign them.
#[derive(Clone)]
pub struct ReleaseConfig {
	pub feed_id: String,
	pub manifest_url: Url,
	pub envelope_url: Url,
	pub registry: Arc<KeyRegistry>,
	pub channel: String,
	/// The version of the running launcher.
	pub current_version: String,
	pub os: &'static str,
	pub arch: &'static str,
	/// Where the launcher is installed. `None` works it out from the running
	/// program; tests supply a folder.
	pub layout: Option<InstallLayout>,
}

impl ReleaseConfig {
	/// The official releases, trusted through the compiled-in release keys.
	pub fn production() -> Self {
		let base = |path: &str| {
			Url::parse(&format!("https://remade.wiki/data/{path}"))
				.expect("the release URLs are valid")
		};

		Self {
			feed_id: product::RELEASE_FEED_ID.to_owned(),
			manifest_url: base("launcher-release.json"),
			envelope_url: base("launcher-release.sig"),
			registry: Arc::new(KeyRegistry::production_releases()),
			channel: "stable".to_owned(),
			current_version: product::VERSION.to_owned(),
			os: std::env::consts::OS,
			arch: std::env::consts::ARCH,
			layout: None,
		}
	}
}

/// A launcher release that is newer than the running one.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LauncherRelease {
	pub version: String,
	pub notes_url: Option<String>,
	/// The download size for this system, when the release has a file for it.
	pub size: Option<u64>,
}

/// Why the launcher cannot update itself here.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Blocker {
	/// This build has no release key, so it cannot tell a real release from
	/// a fake one.
	NoKeys,
	/// A package manager, app bundle, or sandbox owns the files.
	ManagedBy(String),
	/// The release has no file for this system.
	NoFileForThisSystem,
}

impl Blocker {
	pub fn message(&self) -> String {
		match self {
			Self::NoKeys => "This build has no launcher release key, so it cannot check a new launcher is genuine.".into(),
			Self::ManagedBy(by) => format!("The launcher was installed by {by}, which updates it. Update it there."),
			Self::NoFileForThisSystem => "The newest launcher has no download for this system yet.".into(),
		}
	}

	fn error(&self) -> LauncherError {
		LauncherError::new(ErrorCode::SelfUpdateUnavailable, self.message())
	}
}

/// What looking for a new launcher found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LauncherCheck {
	/// The setting is Off, so nothing was looked up.
	Off,
	Blocked(Blocker),
	UpToDate,
	Available(LauncherRelease),
}

/// A downloaded, checked launcher waiting to be switched in.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StagedLauncher {
	pub version: String,
	pub revision: u64,
	pub staged_at: i64,
	files: Vec<StagedFile>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct StagedFile {
	name: String,
	sha256: String,
}

/// The files kept for one step back.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Previous {
	version: String,
	files: Vec<StagedFile>,
}

/// A switch that happened.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppliedLauncher {
	pub from: String,
	pub to: String,
	pub at: i64,
}

/// Everything known about the launcher's own updates, without going online.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LauncherUpdateReport {
	pub current: String,
	pub mode: LauncherUpdates,
	pub checked_at: Option<i64>,
	pub available: Option<LauncherRelease>,
	pub staged: Option<StagedLauncher>,
	/// The latest switch, so the launcher can say what it updated to.
	pub applied: Option<AppliedLauncher>,
	/// A version is kept that the launcher can go back to.
	pub can_roll_back: bool,
	pub blocker: Option<Blocker>,
}

/// What an automatic cycle did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LauncherUpdateOutcome {
	/// Nothing to do: off, too soon, or already current.
	Idle,
	Blocked(Blocker),
	/// A newer launcher exists and was not downloaded.
	Available(LauncherRelease),
	/// A newer launcher is downloaded and will be used after the next close.
	Staged(StagedLauncher),
}

fn rejected(error: &CatalogError) -> LauncherError {
	let message = if error.kind().is_security_failure() {
		"The launcher release did not pass its security checks, so it was ignored."
	} else {
		"The launcher release information could not be used."
	};

	LauncherError::new(ErrorCode::ReleaseRejected, message)
}

fn read_json<T: for<'a> Deserialize<'a>>(text: Option<String>) -> Option<T> {
	text.filter(|text| !text.is_empty())
		.and_then(|text| serde_json::from_str(&text).ok())
}

fn to_json<T: Serialize>(value: &T) -> String {
	serde_json::to_string(value).unwrap_or_default()
}

async fn blocking<T: Send + 'static>(
	work: impl FnOnce() -> Result<T, LauncherError> + Send + 'static,
) -> Result<T, LauncherError> {
	tokio::task::spawn_blocking(work).await.map_err(|error| {
		LauncherError::new(
			ErrorCode::InternalInvariant,
			"The launcher update could not finish.",
		)
		.with_source(error)
	})?
}

fn storage_error(what: &str, error: std::io::Error) -> LauncherError {
	LauncherError::new(ErrorCode::StorageUnavailable, what.to_owned()).with_source(error)
}

impl Engine {
	fn release_layout(&self) -> InstallLayout {
		self.releases
			.layout
			.clone()
			.unwrap_or_else(InstallLayout::detect)
	}

	fn update_folder(&self) -> PathBuf {
		self.data.root().join("launcher-update")
	}

	fn current_version(&self) -> Result<semver::Version, LauncherError> {
		semver::Version::parse(&self.releases.current_version).map_err(|error| {
			LauncherError::new(
				ErrorCode::InternalInvariant,
				"This launcher's own version could not be read.",
			)
			.with_source(error)
		})
	}

	/// What is known about the launcher's updates, without going online.
	pub async fn launcher_update_report(&self) -> LauncherUpdateReport {
		let mode = self.preferences().await.launcher_updates;
		let current = self.releases.current_version.clone();
		let newer = |version: &str| {
			semver::Version::parse(version)
				.ok()
				.zip(semver::Version::parse(&current).ok())
				.is_some_and(|(found, running)| found > running)
		};

		let available = read_json::<LauncherRelease>(self.state_value(AVAILABLE_KEY).await)
			.filter(|release| newer(&release.version));
		let staged = read_json::<StagedLauncher>(self.state_value(STAGED_KEY).await)
			.filter(|staged| newer(&staged.version));
		let blocker = if self.releases.registry.active_key_count() == 0 {
			Some(Blocker::NoKeys)
		} else if let InstallLayout::Managed { by } = self.release_layout() {
			Some(Blocker::ManagedBy(by))
		} else {
			None
		};

		LauncherUpdateReport {
			current,
			mode,
			checked_at: self
				.state_value(CHECKED_KEY)
				.await
				.and_then(|text| text.parse().ok()),
			available,
			staged,
			applied: read_json(self.state_value(APPLIED_KEY).await),
			can_roll_back: read_json::<Previous>(self.state_value(PREVIOUS_KEY).await).is_some(),
			blocker,
		}
	}

	/// Looks for a newer launcher and says what it found. Nothing is
	/// downloaded.
	///
	/// With `automatic` set, the Off setting is honored and nothing is looked
	/// up. A person asking directly is always answered.
	pub async fn check_launcher_update(
		&self,
		automatic: bool,
	) -> Result<LauncherCheck, LauncherError> {
		if automatic && !self.preferences().await.launcher_updates.checks() {
			return Ok(LauncherCheck::Off);
		}

		if self.releases.registry.active_key_count() == 0 {
			return Ok(LauncherCheck::Blocked(Blocker::NoKeys));
		}

		let release = self.fetch_release().await?;

		self.note_release(&release).await
	}

	/// Decides what a verified release means for the running launcher and
	/// remembers it.
	async fn note_release(
		&self,
		release: &VerifiedRelease,
	) -> Result<LauncherCheck, LauncherError> {
		let _ = self
			.set_state_value(CHECKED_KEY, now_unix().to_string())
			.await;

		if *release.version() <= self.current_version()? {
			let _ = self.set_state_value(AVAILABLE_KEY, String::new()).await;

			return Ok(LauncherCheck::UpToDate);
		}

		let size = release
			.artifact_for(self.releases.os, self.releases.arch)
			.map(|artifact| artifact.size);
		let found = LauncherRelease {
			version: release.version().to_string(),
			notes_url: release.notes_url().map(str::to_owned),
			size,
		};

		let _ = self.set_state_value(AVAILABLE_KEY, to_json(&found)).await;

		Ok(LauncherCheck::Available(found))
	}

	/// Downloads and verifies the signed release information.
	async fn fetch_release(&self) -> Result<VerifiedRelease, LauncherError> {
		let mut last = None;

		for _ in 0..PAIR_ATTEMPTS {
			let (manifest, envelope) = tokio::join!(
				self.http
					.get_bounded(self.releases.manifest_url.clone(), MAX_DOCUMENT_BYTES),
				self.http.get_bounded(
					self.releases.envelope_url.clone(),
					limits::MAX_ENVELOPE_BYTES
				),
			);
			let (manifest, envelope) = (manifest?, envelope?);

			match self
				.accept_release(&manifest, &envelope, OffsetDateTime::now_utc())
				.await
			{
				Ok(release) => return Ok(release),
				Err(error) => last = Some(error),
			}
		}

		Err(last.unwrap_or_else(|| {
			LauncherError::new(
				ErrorCode::ReleaseRejected,
				"The launcher release information could not be used.",
			)
		}))
	}

	/// Verifies a downloaded document and its signatures, and remembers it so
	/// an older one is never accepted later.
	pub(crate) async fn accept_release(
		&self,
		manifest: &[u8],
		envelope: &[u8],
		now: OffsetDateTime,
	) -> Result<VerifiedRelease, LauncherError> {
		let high_water = self.state_value(HIGH_WATER_KEY).await.and_then(|text| {
			let (revision, digest) = text.split_once(':')?;
			let digest: [u8; 32] = hex::decode(digest).ok()?.try_into().ok()?;

			Some(HighWater {
				revision: revision.parse().ok()?,
				digest: Some(digest),
			})
		});
		let release = verify_release(
			manifest,
			envelope,
			&ReleaseContext {
				registry: &self.releases.registry,
				feed_id: &self.releases.feed_id,
				channel: &self.releases.channel,
				now,
				high_water,
			},
		)
		.map_err(|error| {
			tracing::warn!(%error, "a launcher release was refused");

			rejected(&error)
		})?;

		let _ = self
			.set_state_value(
				HIGH_WATER_KEY,
				format!("{}:{}", release.revision(), hex::encode(release.digest())),
			)
			.await;

		Ok(release)
	}

	/// Downloads the newest launcher and keeps it aside, ready to switch in.
	///
	/// Everything is checked before it is kept: the signed release, the size
	/// and SHA-256 of the file, and the contents of the archive.
	pub async fn stage_launcher_update(
		&self,
		progress: &mut (dyn FnMut(u64, Option<u64>) + Send),
	) -> Result<StagedLauncher, LauncherError> {
		if self.releases.registry.active_key_count() == 0 {
			return Err(Blocker::NoKeys.error());
		}

		let release = self.fetch_release().await?;

		self.stage_release(&release, progress).await
	}

	pub(crate) async fn stage_release(
		&self,
		release: &VerifiedRelease,
		progress: &mut (dyn FnMut(u64, Option<u64>) + Send),
	) -> Result<StagedLauncher, LauncherError> {
		let _busy = self.launcher_update.try_lock().map_err(|_| {
			LauncherError::new(
				ErrorCode::OperationInProgress,
				"The launcher is already being updated.",
			)
		})?;

		let executable = match self.release_layout() {
			InstallLayout::Standalone { executable, .. } => executable,
			InstallLayout::Managed { by } => return Err(Blocker::ManagedBy(by).error()),
		};

		if *release.version() <= self.current_version()? {
			return Err(LauncherError::new(
				ErrorCode::SelfUpdateUnavailable,
				"This launcher is already the newest.",
			));
		}

		let artifact = release
			.artifact_for(self.releases.os, self.releases.arch)
			.ok_or_else(|| Blocker::NoFileForThisSystem.error())?
			.clone();

		let folder = self.update_folder().join("staged");
		let archive_path = folder.join(&artifact.name);
		let payload = folder.join("payload");

		{
			let folder = folder.clone();

			blocking(move || {
				let _ = std::fs::remove_dir_all(&folder);

				std::fs::create_dir_all(&folder)
					.map_err(|error| storage_error("The update folder could not be made.", error))
			})
			.await?;
		}

		let outcome = self
			.download_and_unpack(&artifact, &archive_path, &payload, &executable, progress)
			.await;

		// The archive has done its job whether or not it worked.
		let _ = tokio::fs::remove_file(&archive_path).await;

		let files = match outcome {
			Ok(files) => files,
			Err(error) => {
				let _ = tokio::fs::remove_dir_all(&folder).await;

				return Err(error);
			}
		};

		let staged = StagedLauncher {
			version: release.version().to_string(),
			revision: release.revision(),
			staged_at: now_unix(),
			files,
		};

		self.set_state_value(STAGED_KEY, to_json(&staged)).await?;

		Ok(staged)
	}

	async fn download_and_unpack(
		&self,
		artifact: &launcher_catalog::ReleaseArtifact,
		archive_path: &Path,
		payload: &Path,
		executable: &str,
		progress: &mut (dyn FnMut(u64, Option<u64>) + Send),
	) -> Result<Vec<StagedFile>, LauncherError> {
		let url = Url::parse(&artifact.url).map_err(|_| {
			LauncherError::new(
				ErrorCode::ReleaseRejected,
				"The launcher download address is not valid.",
			)
		})?;
		let mut file = tokio::fs::OpenOptions::new()
			.write(true)
			.create_new(true)
			.open(archive_path)
			.await
			.map_err(|error| storage_error("The download could not be saved.", error))?;

		let limit = artifact.size.min(MAX_RELEASE_BYTES);
		let downloaded = self
			.lifecycle()
			.artifacts
			.fetch(url, &mut file, limit, progress)
			.await?;

		drop(file);

		if downloaded.size != artifact.size || downloaded.sha256 != artifact.sha256 {
			return Err(LauncherError::new(
				ErrorCode::ArtifactDigestMismatch,
				"The downloaded launcher does not match its signed checksum, so it was discarded.",
			));
		}

		let archive_path = archive_path.to_owned();
		let payload = payload.to_owned();
		let executable = executable.to_owned();

		blocking(move || unpack(&archive_path, &payload, &executable)).await
	}

	/// Switches a staged launcher in, if there is one.
	///
	/// This waits for any install or removal to finish, so the programs are
	/// never replaced in the middle of one. It does not restart anything: the
	/// running launcher carries on, and the new one starts next time.
	pub async fn apply_launcher_update(&self) -> Result<Option<AppliedLauncher>, LauncherError> {
		let Some(staged) = read_json::<StagedLauncher>(self.state_value(STAGED_KEY).await) else {
			return Ok(None);
		};
		let current = self.current_version()?;

		if semver::Version::parse(&staged.version).is_ok_and(|version| version <= current) {
			self.discard_staged().await;

			return Ok(None);
		}

		let InstallLayout::Standalone { directory, .. } = self.release_layout() else {
			return Ok(None);
		};

		let _operation = crate::lifecycle::acquire_operation_lock(&self.data).await?;
		let root = self.update_folder();
		let source = root.join("staged").join("payload");
		let backup = root.join("previous");
		let files: Vec<Replacement> = staged
			.files
			.iter()
			.map(|file| Replacement {
				name: file.name.clone(),
				sha256: file.sha256.clone(),
			})
			.collect();

		let previous = {
			let backup = backup.clone();
			let directory = directory.clone();
			let files = files.clone();

			blocking(move || {
				let _ = std::fs::remove_dir_all(&backup);

				let mut old = Vec::new();

				for file in &files {
					let found = hash_file(&directory.join(&file.name)).map_err(|error| {
						storage_error("The current launcher could not be read.", error)
					})?;

					old.push(StagedFile {
						name: file.name.clone(),
						sha256: found,
					});
				}

				replace_files(&source, &directory, &backup, &files)?;

				Ok(old)
			})
			.await?
		};

		let applied = AppliedLauncher {
			from: self.releases.current_version.clone(),
			to: staged.version.clone(),
			at: now_unix(),
		};

		self.set_state_value(
			PREVIOUS_KEY,
			to_json(&Previous {
				version: self.releases.current_version.clone(),
				files: previous,
			}),
		)
		.await?;
		self.set_state_value(APPLIED_KEY, to_json(&applied)).await?;
		self.discard_staged().await;

		Ok(Some(applied))
	}

	async fn discard_staged(&self) {
		let _ = self.set_state_value(STAGED_KEY, String::new()).await;
		let _ = tokio::fs::remove_dir_all(self.update_folder().join("staged")).await;
	}

	/// Puts back the launcher that was running before the last switch.
	pub async fn roll_back_launcher(&self) -> Result<AppliedLauncher, LauncherError> {
		let previous =
			read_json::<Previous>(self.state_value(PREVIOUS_KEY).await).ok_or_else(|| {
				LauncherError::new(
					ErrorCode::SelfUpdateUnavailable,
					"There is no earlier launcher to go back to.",
				)
			})?;
		let InstallLayout::Standalone { directory, .. } = self.release_layout() else {
			return Err(LauncherError::new(
				ErrorCode::SelfUpdateUnavailable,
				"The launcher's files belong to something else, so they were left alone.",
			));
		};

		let _operation = crate::lifecycle::acquire_operation_lock(&self.data).await?;
		let root = self.update_folder();
		let source = root.join("previous");
		let aside = root.join("replaced");
		let files: Vec<Replacement> = previous
			.files
			.iter()
			.map(|file| Replacement {
				name: file.name.clone(),
				sha256: file.sha256.clone(),
			})
			.collect();

		blocking(move || {
			let _ = std::fs::remove_dir_all(&aside);

			replace_files(&source, &directory, &aside, &files)
		})
		.await?;

		let applied = AppliedLauncher {
			from: self.releases.current_version.clone(),
			to: previous.version,
			at: now_unix(),
		};

		self.set_state_value(PREVIOUS_KEY, String::new()).await?;
		self.set_state_value(APPLIED_KEY, to_json(&applied)).await?;
		self.discard_staged().await;

		Ok(applied)
	}

	/// What the launcher does for itself at startup, according to its
	/// setting: nothing when Off, a quiet look when Tell me, and a download
	/// when Automatic. It never switches files: that waits for the launcher
	/// to close.
	pub async fn launcher_update_cycle(&self) -> LauncherUpdateOutcome {
		let mode = self.preferences().await.launcher_updates;

		if !mode.checks() || self.releases.registry.active_key_count() == 0 {
			return LauncherUpdateOutcome::Idle;
		}

		let report = self.launcher_update_report().await;

		if let Some(staged) = report.staged {
			return LauncherUpdateOutcome::Staged(staged);
		}

		let recently = report
			.checked_at
			.is_some_and(|at| now_unix() - at < CHECK_INTERVAL_SECONDS);

		if recently && report.available.is_none() {
			return LauncherUpdateOutcome::Idle;
		}

		let release = match self.fetch_release().await {
			Ok(release) => release,
			Err(error) => {
				tracing::info!(%error, "the launcher could not look for a new release");

				return match report.available {
					Some(found) => LauncherUpdateOutcome::Available(found),
					None => LauncherUpdateOutcome::Idle,
				};
			}
		};

		let found = match self.note_release(&release).await {
			Ok(LauncherCheck::Available(found)) => found,
			_ => return LauncherUpdateOutcome::Idle,
		};

		if let InstallLayout::Managed { by } = self.release_layout() {
			return if mode.downloads() {
				LauncherUpdateOutcome::Blocked(Blocker::ManagedBy(by))
			} else {
				LauncherUpdateOutcome::Available(found)
			};
		}

		if !mode.downloads() {
			return LauncherUpdateOutcome::Available(found);
		}

		match self.stage_release(&release, &mut |_, _| {}).await {
			Ok(staged) => LauncherUpdateOutcome::Staged(staged),
			Err(error) => {
				tracing::info!(%error, "the new launcher could not be downloaded");

				LauncherUpdateOutcome::Available(found)
			}
		}
	}
}

/// Opens the downloaded archive and keeps the launcher's own programs from
/// it. The archive holds one folder, as the release script makes it.
fn unpack(
	archive: &Path,
	payload: &Path,
	executable: &str,
) -> Result<Vec<StagedFile>, LauncherError> {
	let unsafe_archive = |why: &str| LauncherError::new(ErrorCode::PackageUnsafe, why.to_owned());
	let format = detect_magic(archive)
		.map_err(|_| unsafe_archive("The launcher download could not be read."))?
		.ok_or_else(|| unsafe_archive("The launcher download is not an archive."))?;
	let inventory = inspect(
		archive,
		format,
		1,
		Limits {
			max_entries: 64,
			max_total_bytes: 1024 * 1024 * 1024,
			max_file_bytes: MAX_RELEASE_BYTES,
			..Limits::default()
		},
	)
	.map_err(|_| unsafe_archive("The launcher download is not a safe archive."))?;
	let owned = extract(archive, &inventory, payload)
		.map_err(|_| unsafe_archive("The launcher download could not be unpacked."))?;

	let mut files = Vec::new();

	for file in owned {
		let name = file.path.as_str();
		let known = name.strip_suffix(".exe").unwrap_or(name);

		if file.kind != launcher_packages::OwnedKind::File || !EXECUTABLES.contains(&known) {
			continue;
		}

		if files
			.iter()
			.any(|existing: &StagedFile| existing.name == name)
		{
			return Err(unsafe_archive("The launcher download repeats a program."));
		}

		files.push(StagedFile {
			name: file.path.clone(),
			sha256: file.sha256.clone(),
		});
	}

	// The running program has to be in it. Otherwise the others would move on
	// to a new version while this one stayed behind.
	if !files.iter().any(|file| file.name == executable) {
		return Err(LauncherError::new(
			ErrorCode::PackageUnsupported,
			"The launcher download does not contain the program that is running.",
		));
	}

	Ok(files)
}

#[cfg(test)]
mod tests;
