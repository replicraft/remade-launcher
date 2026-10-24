//! Writing an inspected archive to disk.

use std::fs::File;
use std::io::{BufReader, Read, Write};
use std::path::Path;

use cap_std::ambient_authority;
use cap_std::fs::{Dir, OpenOptions};
use flate2::read::GzDecoder;
use rayon::prelude::*;
use sha2::{Digest, Sha256};

use crate::error::PackageError;
use crate::format::Format;
use crate::inspect::{Entry, EntryKind, Inventory};
use crate::path::normalize;

/// What the launcher created, recorded so removal never has to guess.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OwnedFile {
	pub path: String,
	pub kind: OwnedKind,
	pub size: u64,
	/// SHA-256 of the file as written. Empty for directories and links.
	pub sha256: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OwnedKind {
	File,
	Directory,
	Symlink,
}

/// Extracts `inventory`'s entries from `archive` into `destination`.
///
/// `destination` must be an empty directory. Writes go through a capability
/// handle, so no entry can land outside it, and files are created with
/// `create_new`, so nothing is overwritten.
pub fn extract(
	archive: &Path,
	inventory: &Inventory,
	destination: &Path,
) -> Result<Vec<OwnedFile>, PackageError> {
	std::fs::create_dir_all(destination).map_err(PackageError::io)?;

	if std::fs::read_dir(destination)
		.map_err(PackageError::io)?
		.next()
		.is_some()
	{
		return Err(PackageError::Unsupported(
			"the destination is not empty".into(),
		));
	}

	let root = Dir::open_ambient_dir(destination, ambient_authority()).map_err(PackageError::io)?;

	make_directories(&root, &inventory.entries)?;

	let mut owned = match inventory.format {
		Format::Zip => extract_zip_files(archive, inventory, &root)?,
		Format::TarGz => extract_tar_files(archive, inventory, &root)?,
	};

	// Links come last, so no file write can ever pass through one.
	for entry in &inventory.entries {
		if let EntryKind::Symlink(target) = &entry.kind {
			create_symlink(&root, target, &entry.path)?;

			owned.push(OwnedFile {
				path: entry.path.clone(),
				kind: OwnedKind::Symlink,
				size: 0,
				sha256: String::new(),
			});
		}
	}

	for entry in &inventory.entries {
		if entry.kind == EntryKind::Directory {
			owned.push(OwnedFile {
				path: entry.path.clone(),
				kind: OwnedKind::Directory,
				size: 0,
				sha256: String::new(),
			});
		}
	}

	owned.sort_by(|a, b| a.path.cmp(&b.path));

	Ok(owned)
}

fn make_directories(root: &Dir, entries: &[Entry]) -> Result<(), PackageError> {
	for entry in entries {
		let directory = match entry.kind {
			EntryKind::Directory => Some(entry.path.as_str()),
			_ => entry.path.rsplit_once('/').map(|(parent, _)| parent),
		};

		if let Some(directory) = directory {
			root.create_dir_all(directory).map_err(PackageError::io)?;
		}
	}

	Ok(())
}

#[cfg(unix)]
fn create_symlink(root: &Dir, target: &str, path: &str) -> Result<(), PackageError> {
	root.symlink(target, path).map_err(PackageError::io)
}

#[cfg(not(unix))]
fn create_symlink(_: &Dir, _: &str, path: &str) -> Result<(), PackageError> {
	Err(PackageError::Unsupported(format!("symbolic link {path}")))
}

/// Copies exactly `entry.size` bytes into a new file and hashes them.
fn write_file(root: &Dir, entry: &Entry, source: &mut dyn Read) -> Result<OwnedFile, PackageError> {
	let mut options = OpenOptions::new();
	options.write(true).create_new(true);

	let mut file = root
		.open_with(&entry.path, &options)
		.map_err(PackageError::io)?;
	let mut hasher = Sha256::new();
	let mut written = 0u64;
	let mut buffer = vec![0u8; 64 * 1024];

	// Read one byte more than declared, to notice an entry that lies.
	let mut limited = source.take(entry.size.saturating_add(1));

	loop {
		let read = limited.read(&mut buffer).map_err(PackageError::io)?;

		if read == 0 {
			break;
		}

		written += read as u64;

		if written > entry.size {
			return Err(PackageError::SizeMismatch(entry.path.clone()));
		}

		hasher.update(&buffer[..read]);
		file.write_all(&buffer[..read]).map_err(PackageError::io)?;
	}

	if written != entry.size {
		return Err(PackageError::SizeMismatch(entry.path.clone()));
	}

	set_mode(&file, entry.executable)?;

	Ok(OwnedFile {
		path: entry.path.clone(),
		kind: OwnedKind::File,
		size: written,
		sha256: hex::encode(hasher.finalize()),
	})
}

/// Plain read/write, plus execute when the archive marked the file
/// executable. Setuid, setgid, and sticky bits are never carried over.
#[cfg(unix)]
fn set_mode(file: &cap_std::fs::File, executable: bool) -> Result<(), PackageError> {
	use cap_std::fs::{Permissions, PermissionsExt};

	let mode = if executable { 0o755 } else { 0o644 };

	file.set_permissions(Permissions::from_mode(mode))
		.map_err(PackageError::io)
}

#[cfg(not(unix))]
fn set_mode(_: &cap_std::fs::File, _: bool) -> Result<(), PackageError> {
	Ok(())
}

fn extract_zip_files(
	archive: &Path,
	inventory: &Inventory,
	root: &Dir,
) -> Result<Vec<OwnedFile>, PackageError> {
	let files: Vec<&Entry> = inventory
		.entries
		.iter()
		.filter(|entry| entry.kind == EntryKind::File)
		.collect();

	// Each worker opens its own handle, so entries decompress in parallel.
	// Chunking keeps the cost of reading the central directory low.
	let chunk = files
		.len()
		.div_ceil(rayon::current_num_threads() * 4)
		.max(32);

	let results: Result<Vec<Vec<OwnedFile>>, PackageError> = files
		.par_chunks(chunk)
		.map(|group| {
			let file = File::open(archive).map_err(PackageError::io)?;
			let mut zip = zip::ZipArchive::new(BufReader::new(file))?;
			let mut done = Vec::with_capacity(group.len());

			for entry in group {
				let mut source = zip.by_index(entry.index)?;

				done.push(write_file(root, entry, &mut source)?);
			}

			Ok(done)
		})
		.collect();

	Ok(results?.into_iter().flatten().collect())
}

fn extract_tar_files(
	archive: &Path,
	inventory: &Inventory,
	root: &Dir,
) -> Result<Vec<OwnedFile>, PackageError> {
	let file = File::open(archive).map_err(PackageError::io)?;
	let mut tar = tar::Archive::new(GzDecoder::new(BufReader::new(file)));
	let mut done = Vec::new();

	for (index, entry) in tar.entries().map_err(PackageError::io)?.enumerate() {
		let mut entry = entry.map_err(PackageError::io)?;

		let Some(expected) = inventory
			.entries
			.iter()
			.find(|candidate| candidate.index == index)
		else {
			continue;
		};

		if expected.kind != EntryKind::File {
			continue;
		}

		// The archive on disk must still be the one that was inspected.
		let still_the_same = entry
			.path()
			.ok()
			.and_then(|path| path.to_str().map(str::to_owned))
			.and_then(|name| normalize(&name, inventory.strip_components).ok().flatten())
			.is_some_and(|path| path == expected.path)
			&& entry.header().size().ok() == Some(expected.size);

		if !still_the_same {
			return Err(PackageError::SizeMismatch(expected.path.clone()));
		}

		done.push(write_file(root, expected, &mut entry)?);
	}

	Ok(done)
}
