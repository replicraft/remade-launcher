use std::io::Read;
use std::path::Path;

use crate::error::PackageError;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
	Zip,
	TarGz,
}

/// Reads the file's leading bytes and returns the archive format they show.
///
/// A suffix is only a claim. The adapter is chosen from the catalog, and this
/// check confirms the bytes agree, so a misleading name cannot switch it.
pub fn detect_magic(path: &Path) -> Result<Option<Format>, PackageError> {
	let mut file = std::fs::File::open(path).map_err(PackageError::io)?;
	let mut head = [0u8; 4];
	let read = file.read(&mut head).map_err(PackageError::io)?;

	Ok(match &head[..read] {
		[b'P', b'K', 3, 4, ..] | [b'P', b'K', 5, 6, ..] => Some(Format::Zip),
		[0x1f, 0x8b, ..] => Some(Format::TarGz),
		_ => None,
	})
}
