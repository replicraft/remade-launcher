use thiserror::Error;

#[derive(Debug, Error)]
pub enum PackageError {
	#[error("the file could not be read as an archive")]
	Unreadable(#[source] std::io::Error),
	#[error("the file is not the kind of archive it claims to be")]
	FormatMismatch,
	#[error("the archive has more files than the launcher allows")]
	TooManyEntries,
	#[error("the archive expands to more than the launcher allows")]
	TooLarge,
	#[error("the archive expands far more than its size suggests")]
	CompressionBomb,
	#[error("the archive contains an unsafe path: {0}")]
	UnsafePath(String),
	#[error("the archive contains something the launcher will not install: {0}")]
	Unsupported(String),
	#[error("the archive contains the same file twice: {0}")]
	Duplicate(String),
	#[error("the file the app starts from is not in the archive: {0}")]
	EntryPointMissing(String),
	#[error("an archive entry did not have the size it declared: {0}")]
	SizeMismatch(String),
}

impl PackageError {
	pub(crate) fn io(error: std::io::Error) -> Self {
		Self::Unreadable(error)
	}
}

impl From<zip::result::ZipError> for PackageError {
	fn from(error: zip::result::ZipError) -> Self {
		match error {
			zip::result::ZipError::Io(error) => Self::Unreadable(error),
			other => Self::Unreadable(std::io::Error::other(other.to_string())),
		}
	}
}
