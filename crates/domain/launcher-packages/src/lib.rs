//! Archive handling for downloaded packages.
//!
//! Inspection reads an archive's table of contents and refuses anything
//! unsafe before a single byte is written: escaping paths, device files,
//! hard links, case-colliding names, oversized or bomb-like contents. It
//! never runs the package, and never uses the downloaded app as its own
//! extractor. Extraction then writes only what inspection approved, through
//! a capability handle so a symlink inside the archive cannot redirect a
//! write outside the destination.

mod error;
mod extract;
mod format;
mod inspect;
mod path;

pub use error::PackageError;
pub use extract::{OwnedFile, OwnedKind, extract};
pub use format::{Format, detect_magic};
pub use inspect::{Entry, EntryKind, Inventory, Limits, inspect};
pub use path::is_safe_relative_path;
