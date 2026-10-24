use launcher_model::{ErrorCode, LauncherError};

/// `tokio-rusqlite` closures are generic over their error type, so each one
/// names it through this alias.
pub(crate) type DbResult<T> = Result<T, rusqlite::Error>;

/// Wraps a storage failure. The raw cause stays in the source chain, since it
/// can contain file paths.
pub(crate) fn unavailable(error: impl std::error::Error + Send + Sync + 'static) -> LauncherError {
	LauncherError::new(
		ErrorCode::StorageUnavailable,
		"The launcher's local database could not be used.",
	)
	.with_source(error)
}
