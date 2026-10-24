//! Reading an archive's table of contents, without extracting it.

use std::collections::HashSet;
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;

use flate2::read::GzDecoder;

use crate::error::PackageError;
use crate::format::{Format, detect_magic};
use crate::path::{normalize, symlink_target_is_contained};

const GIB: u64 = 1024 * 1024 * 1024;

/// Resource ceilings for one archive.
#[derive(Clone, Copy, Debug)]
pub struct Limits {
	pub max_entries: usize,
	pub max_total_bytes: u64,
	pub max_file_bytes: u64,
	/// Archives expanding past this multiple of their size are refused, once
	/// they are big enough for the ratio to mean something.
	pub max_expansion_ratio: u64,
}

impl Default for Limits {
	fn default() -> Self {
		Self {
			max_entries: 100_000,
			max_total_bytes: 4 * GIB,
			max_file_bytes: 2 * GIB,
			max_expansion_ratio: 200,
		}
	}
}

/// Below this size, a high ratio is just a small, compressible archive.
const RATIO_FLOOR_BYTES: u64 = 64 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EntryKind {
	File,
	Directory,
	/// A symlink whose target was checked to stay inside the install root.
	Symlink(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
	/// Where the entry lands, relative to the install root.
	pub path: String,
	pub kind: EntryKind,
	pub size: u64,
	pub executable: bool,
	/// Position in the archive, so extraction can confirm it is reading the
	/// same entry that was inspected.
	pub(crate) index: usize,
}

#[derive(Clone, Debug)]
pub struct Inventory {
	pub format: Format,
	/// The component count stripped from every path.
	pub strip_components: u8,
	pub entries: Vec<Entry>,
	pub total_bytes: u64,
}

impl Inventory {
	pub fn has_file(&self, path: &str) -> bool {
		self.entries
			.iter()
			.any(|entry| entry.path == path && entry.kind == EntryKind::File)
	}

	/// True if `path` is an entry or a directory containing entries. An app
	/// bundle such as `Tool.app` is a directory.
	pub fn has_path(&self, path: &str) -> bool {
		let prefix = format!("{path}/");

		self.entries
			.iter()
			.any(|entry| entry.path == path || entry.path.starts_with(&prefix))
	}
}

/// Inspects `archive`, which must really be a `format` archive.
pub fn inspect(
	archive: &Path,
	format: Format,
	strip_components: u8,
	limits: Limits,
) -> Result<Inventory, PackageError> {
	if detect_magic(archive)? != Some(format) {
		return Err(PackageError::FormatMismatch);
	}

	let archive_len = std::fs::metadata(archive).map_err(PackageError::io)?.len();
	let mut collector = Collector::new(limits, strip_components);

	match format {
		Format::Zip => read_zip(archive, &mut collector)?,
		Format::TarGz => read_tar_gz(archive, &mut collector)?,
	}

	collector.finish(format, archive_len)
}

/// Raw facts about one archive entry, before policy.
pub(crate) struct Raw<'a> {
	pub(crate) index: usize,
	pub(crate) name: &'a str,
	pub(crate) kind: RawKind,
	pub(crate) size: u64,
	pub(crate) mode: Option<u32>,
}

pub(crate) enum RawKind {
	File,
	Directory,
	Symlink(String),
	/// Hard link, device, FIFO, socket, or similar.
	Special(&'static str),
}

struct Collector {
	limits: Limits,
	strip: u8,
	entries: Vec<Entry>,
	total: u64,
	seen: HashSet<String>,
	seen_count: usize,
}

impl Collector {
	fn new(limits: Limits, strip: u8) -> Self {
		Self {
			limits,
			strip,
			entries: Vec::new(),
			total: 0,
			seen: HashSet::new(),
			seen_count: 0,
		}
	}

	fn accept(&mut self, raw: Raw<'_>) -> Result<(), PackageError> {
		self.seen_count += 1;

		if self.seen_count > self.limits.max_entries {
			return Err(PackageError::TooManyEntries);
		}

		// Validate before stripping, so an unsafe name is refused even if it
		// would be stripped away.
		let Some(path) = normalize(raw.name, self.strip)? else {
			return Ok(());
		};

		let kind = match raw.kind {
			RawKind::File => EntryKind::File,
			RawKind::Directory => EntryKind::Directory,
			RawKind::Symlink(target) => {
				if !cfg!(unix) {
					return Err(PackageError::Unsupported(format!("symbolic link {path}")));
				}

				if !symlink_target_is_contained(&path, &target) {
					return Err(PackageError::UnsafePath(format!(
						"link {path} points outside the app"
					)));
				}

				EntryKind::Symlink(target)
			}
			RawKind::Special(what) => {
				return Err(PackageError::Unsupported(format!("{what}: {path}")));
			}
		};

		if raw.size > self.limits.max_file_bytes {
			return Err(PackageError::TooLarge);
		}

		self.total = self.total.saturating_add(raw.size);

		if self.total > self.limits.max_total_bytes {
			return Err(PackageError::TooLarge);
		}

		// Names that differ only by case collide on common filesystems.
		if !self.seen.insert(path.to_lowercase()) {
			return Err(PackageError::Duplicate(path));
		}

		self.entries.push(Entry {
			path,
			kind,
			size: raw.size,
			executable: raw.mode.is_some_and(|mode| mode & 0o111 != 0),
			index: raw.index,
		});

		Ok(())
	}

	fn finish(self, format: Format, archive_len: u64) -> Result<Inventory, PackageError> {
		let ratio_exceeded = self.total > RATIO_FLOOR_BYTES
			&& self.total / archive_len.max(1) > self.limits.max_expansion_ratio;

		if ratio_exceeded {
			return Err(PackageError::CompressionBomb);
		}

		// A file cannot also be a directory that holds other entries.
		let files: HashSet<&str> = self
			.entries
			.iter()
			.filter(|entry| !matches!(entry.kind, EntryKind::Directory))
			.map(|entry| entry.path.as_str())
			.collect();

		for entry in &self.entries {
			let mut parent = entry.path.as_str();

			while let Some((head, _)) = parent.rsplit_once('/') {
				if files.contains(head) {
					return Err(PackageError::Duplicate(head.to_owned()));
				}

				parent = head;
			}
		}

		Ok(Inventory {
			format,
			strip_components: self.strip,
			entries: self.entries,
			total_bytes: self.total,
		})
	}
}

fn read_zip(archive_path: &Path, collector: &mut Collector) -> Result<(), PackageError> {
	let file = File::open(archive_path).map_err(PackageError::io)?;
	let mut archive = zip::ZipArchive::new(BufReader::new(file))?;

	if archive.len() > collector.limits.max_entries {
		return Err(PackageError::TooManyEntries);
	}

	for index in 0..archive.len() {
		let mut entry = archive.by_index(index)?;

		if entry.encrypted() {
			return Err(PackageError::Unsupported("an encrypted entry".into()));
		}

		// A name that is not valid UTF-8 or fails the zip crate's own checks
		// is refused rather than guessed at.
		let name = entry
			.name()
			.map_err(|_| PackageError::UnsafePath("an unreadable entry name".into()))?
			.into_owned();
		let mode = entry.unix_mode();
		let size = entry.size();

		let kind = match mode.map(|mode| mode & 0o170_000) {
			Some(0o120_000) => {
				let mut target = String::new();

				(&mut entry)
					.take(4097)
					.read_to_string(&mut target)
					.map_err(|_| PackageError::Unsupported(format!("unreadable link {name}")))?;

				if target.len() > 4096 {
					return Err(PackageError::Unsupported(format!("oversized link {name}")));
				}

				RawKind::Symlink(target)
			}
			Some(0o020_000 | 0o060_000 | 0o010_000 | 0o140_000) => {
				RawKind::Special("a device, pipe, or socket")
			}
			_ if entry.is_dir() => RawKind::Directory,
			_ => RawKind::File,
		};

		collector.accept(Raw {
			index,
			name: &name,
			kind,
			size: if entry.is_dir() { 0 } else { size },
			mode,
		})?;
	}

	Ok(())
}

fn read_tar_gz(archive_path: &Path, collector: &mut Collector) -> Result<(), PackageError> {
	let file = File::open(archive_path).map_err(PackageError::io)?;

	// A stream limit stops a gzip bomb that lies about nothing in its headers.
	let budget = collector
		.limits
		.max_total_bytes
		.saturating_add(collector.limits.max_entries as u64 * 1024)
		.saturating_add(1024 * 1024);
	let decoder = GzDecoder::new(BufReader::new(file)).take(budget);
	let mut archive = tar::Archive::new(decoder);

	for (index, entry) in archive.entries().map_err(PackageError::io)?.enumerate() {
		let entry = entry.map_err(PackageError::io)?;
		let header = entry.header();
		let entry_type = header.entry_type();

		let kind = match entry_type {
			tar::EntryType::Regular | tar::EntryType::Continuous => RawKind::File,
			tar::EntryType::Directory => RawKind::Directory,
			tar::EntryType::Symlink => {
				let target = entry
					.link_name()
					.map_err(PackageError::io)?
					.and_then(|target| target.to_str().map(str::to_owned))
					.ok_or_else(|| {
						PackageError::Unsupported("a link without a readable target".into())
					})?;

				RawKind::Symlink(target)
			}
			tar::EntryType::Link => RawKind::Special("a hard link"),
			// Metadata records the tar crate does not turn into files.
			tar::EntryType::XGlobalHeader | tar::EntryType::XHeader => continue,
			_ => RawKind::Special("a device, pipe, or unknown record"),
		};

		let name = entry
			.path()
			.map_err(PackageError::io)?
			.to_str()
			.map(str::to_owned)
			.ok_or_else(|| PackageError::UnsafePath("a name that is not valid UTF-8".into()))?;

		let size = if matches!(kind, RawKind::File) {
			header.size().map_err(PackageError::io)?
		} else {
			0
		};

		collector.accept(Raw {
			index,
			name: &name,
			kind,
			size,
			mode: header.mode().ok(),
		})?;
	}

	Ok(())
}
