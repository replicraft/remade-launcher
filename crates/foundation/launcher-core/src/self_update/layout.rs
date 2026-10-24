//! Where the running launcher lives, and how to switch its files safely.
//!
//! The launcher only replaces itself when it is a plain folder of executables
//! that the current user owns. Anything installed by a package manager, an
//! app bundle, or a sandbox belongs to whatever installed it, so those only
//! ever get a "an update is available" message.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use launcher_model::{ErrorCode, LauncherError};
use sha2::{Digest, Sha256};

/// The launcher's own programs. Only files with these names are ever
/// replaced, and only if they are already there.
pub(crate) const EXECUTABLES: [&str; 4] = [
	"remade-launcher",
	"remade",
	"remade-launcher-daemon",
	"remade-launcher-mcp",
];

/// How the running launcher is installed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InstallLayout {
	/// A folder of executables the launcher may replace.
	Standalone {
		directory: PathBuf,
		/// The running program's file name.
		executable: String,
	},
	/// A macOS app bundle. It is replaced whole, because changing a file
	/// inside would break its code signature.
	AppBundle {
		bundle: PathBuf,
		/// The running program's file name, inside `Contents/MacOS`.
		executable: String,
	},
	/// Something else owns the files, so the launcher leaves them alone.
	Managed { by: String },
}

impl InstallLayout {
	/// Works it out from the running executable.
	pub fn detect() -> Self {
		let Ok(exe) = std::env::current_exe() else {
			return Self::managed("an unknown location");
		};

		Self::for_executable(&exe, |name| std::env::var_os(name).is_some())
	}

	/// The same decision from a path and the environment, so it can be tested.
	pub(crate) fn for_executable(exe: &Path, has_variable: impl Fn(&str) -> bool) -> Self {
		let text = exe.to_string_lossy().replace('\\', "/");

		if let Some(bundle) = Self::bundle_of(exe) {
			let executable = exe
				.file_name()
				.map(|name| name.to_string_lossy().into_owned())
				.unwrap_or_default();

			return Self::AppBundle { bundle, executable };
		}

		let owners: [(&str, &str); 8] = [
			("/Cellar/", "Homebrew"),
			("/homebrew/", "Homebrew"),
			("/linuxbrew/", "Homebrew"),
			("/WindowsApps/", "the Microsoft Store"),
			("/nix/store/", "Nix"),
			("/snap/", "Snap"),
			("/usr/bin/", "the system package manager"),
			("/usr/lib/", "the system package manager"),
		];

		if let Some((_, by)) = owners.iter().find(|(marker, _)| text.contains(marker)) {
			return Self::managed(by);
		}

		if has_variable("APPIMAGE") {
			return Self::managed("the AppImage");
		}

		if has_variable("FLATPAK_ID") {
			return Self::managed("Flatpak");
		}

		let name = exe
			.file_name()
			.map(|name| name.to_string_lossy().into_owned());

		match (exe.parent(), name) {
			(Some(directory), Some(executable)) if directory.is_dir() => Self::Standalone {
				directory: directory.to_owned(),
				executable,
			},
			_ => Self::managed("an unknown location"),
		}
	}

	/// `Remade Launcher.app` for `.../Remade Launcher.app/Contents/MacOS/x`.
	fn bundle_of(exe: &Path) -> Option<PathBuf> {
		let macos = exe.parent()?;
		let contents = macos.parent()?;
		let bundle = contents.parent()?;

		(macos.file_name()? == "MacOS"
			&& contents.file_name()? == "Contents"
			&& bundle.extension()? == "app")
			.then(|| bundle.to_owned())
	}

	fn managed(by: &str) -> Self {
		Self::Managed { by: by.to_owned() }
	}
}

fn io_failed(what: &str, error: io::Error) -> LauncherError {
	LauncherError::new(ErrorCode::StorageUnavailable, what.to_owned()).with_source(error)
}

/// SHA-256 of a file, as lowercase hex.
pub(crate) fn hash_file(path: &Path) -> io::Result<String> {
	let mut file = fs::File::open(path)?;
	let mut hasher = Sha256::new();
	let mut buffer = vec![0; 64 * 1024];

	loop {
		let read = io::Read::read(&mut file, &mut buffer)?;

		if read == 0 {
			break;
		}

		hasher.update(&buffer[..read]);
	}

	Ok(hex::encode(hasher.finalize()))
}

/// One file to put in place.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Replacement {
	pub(crate) name: String,
	pub(crate) sha256: String,
}

/// Puts the files from `source` into `directory`, replacing the same-named
/// files that are already there, and keeps the old ones in `backup`.
///
/// Either every file is replaced or none are. Each file is written beside its
/// target first and moved in with a rename, so a crash never leaves half a
/// program behind. If a step fails, the files already switched are put back.
pub(crate) fn replace_files(
	source: &Path,
	directory: &Path,
	backup: &Path,
	files: &[Replacement],
) -> Result<(), LauncherError> {
	sweep_leftovers(directory);

	for file in files {
		let staged = source.join(&file.name);
		let found =
			hash_file(&staged).map_err(|e| io_failed("A new file could not be read.", e))?;

		if found != file.sha256 {
			return Err(LauncherError::new(
				ErrorCode::ArtifactDigestMismatch,
				"A downloaded file changed after it was checked, so nothing was replaced.",
			));
		}

		if !directory.join(&file.name).is_file() {
			return Err(LauncherError::new(
				ErrorCode::InstallationChanged,
				"The launcher's files are not where they were, so nothing was replaced.",
			));
		}
	}

	// Keep the current versions first, so there is always a way back.
	fs::create_dir_all(backup).map_err(|e| io_failed("The backup folder could not be made.", e))?;

	for file in files {
		fs::copy(directory.join(&file.name), backup.join(&file.name))
			.map_err(|e| io_failed("The current launcher could not be backed up.", e))?;
	}

	let mut switched: Vec<&Replacement> = Vec::new();

	for file in files {
		match switch_in(source, directory, &file.name) {
			Ok(()) => switched.push(file),
			Err(error) => {
				for done in switched.into_iter().rev() {
					put_back(directory, &done.name, backup);
				}

				return Err(error);
			}
		}
	}

	// A last look: what is installed is what was checked.
	for file in files {
		let installed = hash_file(&directory.join(&file.name)).ok();

		if installed.as_deref() != Some(file.sha256.as_str()) {
			for done in files {
				put_back(directory, &done.name, backup);
			}

			return Err(LauncherError::new(
				ErrorCode::InstallationChanged,
				"The new launcher did not land intact, so the old one was put back.",
			));
		}
	}

	sweep_leftovers(directory);

	Ok(())
}

/// A fingerprint of a whole folder: every file's name, executable bit, and
/// SHA-256, and every link's target. Two folders with the same fingerprint are
/// the same bundle.
pub(crate) fn tree_digest(root: &Path) -> io::Result<String> {
	let mut lines = Vec::new();

	collect(root, root, &mut lines)?;
	lines.sort();

	Ok(hex::encode(Sha256::digest(lines.join("\n").as_bytes())))
}

fn collect(root: &Path, folder: &Path, lines: &mut Vec<String>) -> io::Result<()> {
	for entry in fs::read_dir(folder)? {
		let path = entry?.path();
		let relative = path
			.strip_prefix(root)
			.map_err(io::Error::other)?
			.to_string_lossy()
			.replace('\\', "/");
		let kind = fs::symlink_metadata(&path)?;

		if kind.is_symlink() {
			lines.push(format!(
				"L {relative} -> {}",
				fs::read_link(&path)?.display()
			));
		} else if kind.is_dir() {
			lines.push(format!("D {relative}"));
			collect(root, &path, lines)?;
		} else {
			lines.push(format!(
				"F {relative} {} {}",
				u8::from(is_executable(&kind)),
				hash_file(&path)?
			));
		}
	}

	Ok(())
}

#[cfg(unix)]
fn is_executable(metadata: &fs::Metadata) -> bool {
	use std::os::unix::fs::PermissionsExt;

	metadata.permissions().mode() & 0o111 != 0
}

#[cfg(not(unix))]
fn is_executable(_metadata: &fs::Metadata) -> bool {
	false
}

/// Copies a folder with its links and permissions.
fn copy_tree(from: &Path, to: &Path) -> io::Result<()> {
	fs::create_dir_all(to)?;

	for entry in fs::read_dir(from)? {
		let entry = entry?;
		let (source, target) = (entry.path(), to.join(entry.file_name()));
		let kind = fs::symlink_metadata(&source)?;

		if kind.is_symlink() {
			link(&fs::read_link(&source)?, &target)?;
		} else if kind.is_dir() {
			copy_tree(&source, &target)?;
		} else {
			fs::copy(&source, &target)?;
		}
	}

	Ok(())
}

#[cfg(unix)]
fn link(target: &Path, at: &Path) -> io::Result<()> {
	std::os::unix::fs::symlink(target, at)
}

#[cfg(not(unix))]
fn link(_target: &Path, _at: &Path) -> io::Result<()> {
	Err(io::Error::other("links are not copied on this system"))
}

fn hidden(bundle: &Path, tag: &str) -> PathBuf {
	let name = bundle
		.file_name()
		.map(|name| name.to_string_lossy().into_owned())
		.unwrap_or_default();

	bundle.with_file_name(format!(".{name}.{tag}"))
}

/// Replaces an app bundle with the one in `source`, keeping the old one in
/// `backup`. The new bundle is copied beside the old, checked against
/// `expected`, and moved in with a rename, so the app is never half-written.
pub(crate) fn replace_bundle(
	source: &Path,
	bundle: &Path,
	backup: &Path,
	expected: &str,
) -> Result<(), LauncherError> {
	let found = tree_digest(source).map_err(|e| io_failed("The new app could not be read.", e))?;

	if found != expected {
		return Err(LauncherError::new(
			ErrorCode::ArtifactDigestMismatch,
			"The downloaded app changed after it was checked, so nothing was replaced.",
		));
	}

	if !bundle.is_dir() {
		return Err(LauncherError::new(
			ErrorCode::InstallationChanged,
			"The launcher's app is not where it was, so nothing was replaced.",
		));
	}

	let (fresh, aside) = (hidden(bundle, "new"), hidden(bundle, "old"));

	let _ = fs::remove_dir_all(&fresh);
	let _ = fs::remove_dir_all(&aside);

	let prepared = copy_tree(source, &fresh)
		.and_then(|()| tree_digest(&fresh))
		.map_err(|e| io_failed("The new app could not be written.", e));

	match prepared {
		Ok(copied) if copied == expected => {}
		Ok(_) => {
			let _ = fs::remove_dir_all(&fresh);

			return Err(LauncherError::new(
				ErrorCode::ArtifactDigestMismatch,
				"The new app did not copy intact, so nothing was replaced.",
			));
		}
		Err(error) => {
			let _ = fs::remove_dir_all(&fresh);

			return Err(error);
		}
	}

	// The current app is kept first, so there is always a way back.
	let name = bundle.file_name().unwrap_or_default();

	if let Err(error) = fs::remove_dir_all(backup)
		.or_else(|e| {
			if e.kind() == io::ErrorKind::NotFound {
				Ok(())
			} else {
				Err(e)
			}
		})
		.and_then(|()| copy_tree(bundle, &backup.join(name)))
	{
		let _ = fs::remove_dir_all(&fresh);

		return Err(io_failed("The current app could not be backed up.", error));
	}

	fs::rename(bundle, &aside).map_err(|e| {
		let _ = fs::remove_dir_all(&fresh);

		io_failed("The current app could not be moved aside.", e)
	})?;

	if let Err(error) = fs::rename(&fresh, bundle) {
		let _ = fs::rename(&aside, bundle);
		let _ = fs::remove_dir_all(&fresh);

		return Err(io_failed("The new app could not be put in place.", error));
	}

	let _ = fs::remove_dir_all(&aside);

	Ok(())
}

fn old_name(directory: &Path, name: &str) -> PathBuf {
	directory.join(format!(".{name}.old"))
}

fn new_name(directory: &Path, name: &str) -> PathBuf {
	directory.join(format!(".{name}.new"))
}

/// Copies the new file beside the old one, then swaps them by renaming.
///
/// The running program can be renamed on every system, even where it cannot
/// be overwritten, so the old file is moved aside rather than deleted.
fn switch_in(source: &Path, directory: &Path, name: &str) -> Result<(), LauncherError> {
	let target = directory.join(name);
	let fresh = new_name(directory, name);
	let aside = old_name(directory, name);

	let _ = fs::remove_file(&fresh);
	let _ = fs::remove_file(&aside);

	fs::copy(source.join(name), &fresh)
		.map_err(|e| io_failed("The new launcher could not be written.", e))?;
	make_executable(&fresh)?;

	fs::rename(&target, &aside).map_err(|e| {
		let _ = fs::remove_file(&fresh);

		io_failed("The current launcher could not be moved aside.", e)
	})?;

	if let Err(error) = fs::rename(&fresh, &target) {
		let _ = fs::rename(&aside, &target);
		let _ = fs::remove_file(&fresh);

		return Err(io_failed(
			"The new launcher could not be put in place.",
			error,
		));
	}

	Ok(())
}

/// Restores one file from the backup, whatever state the swap left it in.
fn put_back(directory: &Path, name: &str, backup: &Path) {
	let target = directory.join(name);
	let aside = old_name(directory, name);

	if aside.is_file() {
		let _ = fs::remove_file(&target);

		if fs::rename(&aside, &target).is_ok() {
			return;
		}
	}

	let _ = fs::copy(backup.join(name), &target);
	let _ = make_executable(&target);
}

/// Removes the moved-aside copies. On Windows the running program's old copy
/// cannot be deleted yet, so this tries again the next time.
fn sweep_leftovers(directory: &Path) {
	for name in EXECUTABLES {
		let _ = fs::remove_file(old_name(directory, name));
		let _ = fs::remove_file(new_name(directory, name));
		let exe = format!("{name}.exe");
		let _ = fs::remove_file(old_name(directory, &exe));
		let _ = fs::remove_file(new_name(directory, &exe));
	}
}

#[cfg(unix)]
fn make_executable(path: &Path) -> Result<(), LauncherError> {
	use std::os::unix::fs::PermissionsExt;

	fs::set_permissions(path, fs::Permissions::from_mode(0o755))
		.map_err(|e| io_failed("The new launcher could not be made runnable.", e))
}

#[cfg(not(unix))]
fn make_executable(_path: &Path) -> Result<(), LauncherError> {
	Ok(())
}

#[cfg(test)]
mod tests {
	use super::*;

	fn write(directory: &Path, name: &str, text: &str) -> Replacement {
		fs::write(directory.join(name), text).unwrap();

		Replacement {
			name: name.to_owned(),
			sha256: hex::encode(Sha256::digest(text.as_bytes())),
		}
	}

	fn read(directory: &Path, name: &str) -> String {
		fs::read_to_string(directory.join(name)).unwrap()
	}

	struct Folders {
		_root: tempfile::TempDir,
		install: PathBuf,
		source: PathBuf,
		backup: PathBuf,
	}

	fn folders() -> Folders {
		let root = tempfile::tempdir().unwrap();
		let install = root.path().join("install");
		let source = root.path().join("source");
		let backup = root.path().join("backup");

		fs::create_dir_all(&install).unwrap();
		fs::create_dir_all(&source).unwrap();

		Folders {
			_root: root,
			install,
			source,
			backup,
		}
	}

	fn detect(path: &str, variables: &[&str]) -> InstallLayout {
		InstallLayout::for_executable(Path::new(path), |name| variables.contains(&name))
	}

	#[test]
	fn package_managers_and_bundles_keep_their_own_files() {
		for (path, owner) in [
			("/opt/homebrew/Cellar/remade/1.0/bin/remade", "Homebrew"),
			(
				"C:\\Program Files\\WindowsApps\\x\\remade.exe",
				"the Microsoft Store",
			),
			("/nix/store/abc-remade/bin/remade", "Nix"),
			("/usr/bin/remade", "the system package manager"),
		] {
			assert_eq!(
				detect(path, &[]),
				InstallLayout::Managed { by: owner.into() },
				"{path}"
			);
		}

		assert_eq!(
			detect("/tmp/remade", &["APPIMAGE"]),
			InstallLayout::Managed {
				by: "the AppImage".into()
			}
		);
		assert_eq!(
			detect("/tmp/remade", &["FLATPAK_ID"]),
			InstallLayout::Managed {
				by: "Flatpak".into()
			}
		);
	}

	#[test]
	fn a_plain_folder_can_be_updated() {
		let folders = folders();
		let exe = folders.install.join("remade");

		fs::write(&exe, "x").unwrap();

		assert_eq!(
			InstallLayout::for_executable(&exe, |_| false),
			InstallLayout::Standalone {
				directory: folders.install.clone(),
				executable: "remade".into(),
			}
		);
	}

	#[test]
	fn files_are_replaced_together_and_the_old_ones_kept() {
		let folders = folders();

		write(&folders.install, "remade", "old cli");
		write(&folders.install, "remade-launcher-daemon", "old daemon");
		write(&folders.install, "notes.txt", "mine");

		let files = vec![
			write(&folders.source, "remade", "new cli"),
			write(&folders.source, "remade-launcher-daemon", "new daemon"),
		];

		replace_files(&folders.source, &folders.install, &folders.backup, &files).unwrap();

		assert_eq!(read(&folders.install, "remade"), "new cli");
		assert_eq!(
			read(&folders.install, "remade-launcher-daemon"),
			"new daemon"
		);
		assert_eq!(read(&folders.backup, "remade"), "old cli");
		assert_eq!(read(&folders.install, "notes.txt"), "mine");
		assert!(!old_name(&folders.install, "remade").exists());

		#[cfg(unix)]
		{
			use std::os::unix::fs::PermissionsExt;

			let mode = fs::metadata(folders.install.join("remade"))
				.unwrap()
				.permissions()
				.mode();

			assert_eq!(mode & 0o111, 0o111, "the new program can be run");
		}
	}

	#[test]
	fn a_file_that_changed_after_the_check_stops_everything() {
		let folders = folders();

		write(&folders.install, "remade", "old cli");

		let mut files = vec![write(&folders.source, "remade", "new cli")];

		files[0].sha256 = "0".repeat(64);

		let error =
			replace_files(&folders.source, &folders.install, &folders.backup, &files).unwrap_err();

		assert_eq!(error.code(), ErrorCode::ArtifactDigestMismatch);
		assert_eq!(read(&folders.install, "remade"), "old cli");
	}

	#[test]
	fn a_missing_target_means_nothing_is_touched() {
		let folders = folders();

		write(&folders.install, "remade", "old cli");

		let files = vec![
			write(&folders.source, "remade", "new cli"),
			write(&folders.source, "remade-launcher-mcp", "new mcp"),
		];

		let error =
			replace_files(&folders.source, &folders.install, &folders.backup, &files).unwrap_err();

		assert_eq!(error.code(), ErrorCode::InstallationChanged);
		assert_eq!(read(&folders.install, "remade"), "old cli");
		assert!(!folders.install.join("remade-launcher-mcp").exists());
	}

	#[test]
	fn a_failure_part_way_puts_the_earlier_files_back() {
		let folders = folders();

		write(&folders.install, "remade", "old cli");
		write(&folders.install, "remade-launcher-daemon", "old daemon");

		let files = vec![
			write(&folders.source, "remade", "new cli"),
			write(&folders.source, "remade-launcher-daemon", "new daemon"),
		];

		// The second file cannot be switched: its `.new` name is a folder.
		fs::create_dir_all(new_name(&folders.install, "remade-launcher-daemon")).unwrap();

		assert!(replace_files(&folders.source, &folders.install, &folders.backup, &files).is_err());
		assert_eq!(read(&folders.install, "remade"), "old cli");
		assert_eq!(
			read(&folders.install, "remade-launcher-daemon"),
			"old daemon"
		);
	}

	fn make_bundle(root: &Path, marker: &str) -> PathBuf {
		let bundle = root.join("Remade Launcher.app");
		let macos = bundle.join("Contents/MacOS");

		fs::create_dir_all(&macos).unwrap();
		fs::write(macos.join("remade-launcher"), marker).unwrap();
		fs::write(
			bundle.join("Contents/Info.plist"),
			format!("<plist>{marker}</plist>"),
		)
		.unwrap();

		#[cfg(unix)]
		{
			use std::os::unix::fs::PermissionsExt;

			fs::set_permissions(
				macos.join("remade-launcher"),
				fs::Permissions::from_mode(0o755),
			)
			.unwrap();
		}

		bundle
	}

	#[test]
	fn a_bundle_is_recognised_by_its_shape() {
		assert_eq!(
			detect(
				"/Applications/Remade Launcher.app/Contents/MacOS/remade-launcher",
				&[]
			),
			InstallLayout::AppBundle {
				bundle: PathBuf::from("/Applications/Remade Launcher.app"),
				executable: "remade-launcher".into(),
			}
		);
		// A folder that merely has "Contents" in its name is not a bundle.
		assert!(matches!(
			detect("/tmp/Contents/MacOS/remade", &[]),
			InstallLayout::Standalone { .. } | InstallLayout::Managed { .. }
		));
	}

	#[test]
	fn the_fingerprint_changes_with_any_file_or_the_executable_bit() {
		let folders = folders();
		let one = make_bundle(&folders.source, "one");
		let before = tree_digest(&one).unwrap();

		assert_eq!(before, tree_digest(&one).unwrap());

		fs::write(one.join("Contents/Info.plist"), "changed").unwrap();

		assert_ne!(before, tree_digest(&one).unwrap());

		#[cfg(unix)]
		{
			use std::os::unix::fs::PermissionsExt;

			let again = tree_digest(&one).unwrap();

			fs::set_permissions(
				one.join("Contents/MacOS/remade-launcher"),
				fs::Permissions::from_mode(0o644),
			)
			.unwrap();

			assert_ne!(again, tree_digest(&one).unwrap());
		}
	}

	#[test]
	fn a_bundle_is_replaced_whole_and_the_old_one_kept() {
		let folders = folders();
		let old = make_bundle(&folders.install, "old");
		let new = make_bundle(&folders.source, "new");
		let expected = tree_digest(&new).unwrap();

		replace_bundle(&new, &old, &folders.backup, &expected).unwrap();

		assert_eq!(read(&old.join("Contents/MacOS"), "remade-launcher"), "new");
		assert_eq!(
			read(
				&folders.backup.join("Remade Launcher.app/Contents/MacOS"),
				"remade-launcher"
			),
			"old"
		);
		assert!(!hidden(&old, "old").exists() && !hidden(&old, "new").exists());
		assert_eq!(tree_digest(&old).unwrap(), expected);
	}

	#[test]
	fn a_bundle_that_changed_after_the_check_is_not_installed() {
		let folders = folders();
		let old = make_bundle(&folders.install, "old");
		let new = make_bundle(&folders.source, "new");
		let expected = tree_digest(&new).unwrap();

		fs::write(new.join("Contents/Info.plist"), "tampered").unwrap();

		let error = replace_bundle(&new, &old, &folders.backup, &expected).unwrap_err();

		assert_eq!(error.code(), ErrorCode::ArtifactDigestMismatch);
		assert_eq!(read(&old.join("Contents/MacOS"), "remade-launcher"), "old");
	}

	#[test]
	fn a_missing_bundle_is_never_created_from_nothing() {
		let folders = folders();
		let new = make_bundle(&folders.source, "new");
		let expected = tree_digest(&new).unwrap();
		let missing = folders.install.join("Remade Launcher.app");

		let error = replace_bundle(&new, &missing, &folders.backup, &expected).unwrap_err();

		assert_eq!(error.code(), ErrorCode::InstallationChanged);
		assert!(!missing.exists());
	}
}
