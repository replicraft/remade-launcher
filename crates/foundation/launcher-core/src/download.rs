//! Fetching a verified file without installing it.
//!
//! This is the same download an install makes: the same address checks, the
//! same size and checksum checks. It stops there. Nothing is unpacked, nothing
//! is recorded as installed, and nothing is run.

use std::path::{Path, PathBuf};

use launcher_model::{ErrorCode, LauncherError};
use launcher_net::Url;

use crate::engine::Engine;
use crate::prepare::Ready;

/// A hard ceiling, whatever the provider says the file weighs.
const MAX_BYTES: u64 = 2 * 1024 * 1024 * 1024;

/// What was saved.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DownloadedFile {
	pub path: PathBuf,
	pub size: u64,
	/// Lowercase hex SHA-256 of the saved bytes.
	pub sha256: String,
	/// The checksum the file was held to, if one was known up front.
	pub matched_expected_sha256: bool,
}

fn refused(code: ErrorCode, why: &str) -> LauncherError {
	LauncherError::new(code, why.to_owned())
}

/// A file name that cannot be a path: no folders, no dots-only names, and no
/// characters a file system treats specially.
pub(crate) fn safe_file_name(name: &str) -> Option<String> {
	let cleaned: String = name
		.chars()
		.map(|c| {
			if c.is_control() || matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') {
				'_'
			} else {
				c
			}
		})
		.collect();
	let cleaned = cleaned.trim().trim_matches('.').to_owned();

	(!cleaned.is_empty() && cleaned.len() <= 200).then_some(cleaned)
}

impl Engine {
	/// Downloads the file a plan names into `folder`, checks it, and leaves it
	/// there. Refuses to overwrite a file that is already there.
	pub async fn download_artifact(
		&self,
		ready: &Ready,
		folder: &Path,
		progress: &mut (dyn FnMut(u64, Option<u64>) + Send),
	) -> Result<DownloadedFile, LauncherError> {
		let body = ready.plan.body();
		let asset = &body.asset;
		let url = Url::parse(&asset.download_url)
			.ok()
			.filter(|url| url.scheme() == "https" && url.host_str() == Some("github.com"))
			.ok_or_else(|| {
				refused(
					ErrorCode::PackageUnsafe,
					"The download address is not an https github.com release link.",
				)
			})?;
		let name = safe_file_name(&asset.name).ok_or_else(|| {
			refused(
				ErrorCode::PackageUnsafe,
				"The file's name is not safe to save.",
			)
		})?;

		std::fs::create_dir_all(folder).map_err(|error| {
			LauncherError::new(
				ErrorCode::StorageUnavailable,
				"The folder could not be created.",
			)
			.with_source(error)
		})?;

		let target = folder.join(&name);

		if target.exists() {
			return Err(refused(
				ErrorCode::InstallationChanged,
				"A file with that name is already there, so nothing was downloaded.",
			));
		}

		// A partial file never has the final name.
		let partial = folder.join(format!(".{name}.part"));
		let result = self.fetch_checked(url, &partial, ready, progress).await;

		match result {
			Ok(downloaded) => {
				std::fs::rename(&partial, &target).map_err(|error| {
					LauncherError::new(
						ErrorCode::StorageUnavailable,
						"The file could not be saved.",
					)
					.with_source(error)
				})?;

				Ok(DownloadedFile {
					path: target,
					size: downloaded.0,
					sha256: downloaded.1,
					matched_expected_sha256: body.install.expected_sha256.is_some(),
				})
			}
			Err(error) => {
				let _ = std::fs::remove_file(&partial);

				Err(error)
			}
		}
	}

	async fn fetch_checked(
		&self,
		url: Url,
		partial: &Path,
		ready: &Ready,
		progress: &mut (dyn FnMut(u64, Option<u64>) + Send),
	) -> Result<(u64, String), LauncherError> {
		let body = ready.plan.body();
		let mut file = tokio::fs::OpenOptions::new()
			.write(true)
			.create_new(true)
			.open(partial)
			.await
			.map_err(|error| {
				LauncherError::new(
					ErrorCode::StorageUnavailable,
					"The download could not be saved to disk.",
				)
				.with_source(error)
			})?;

		let limit = if body.asset.size > 0 {
			body.asset.size.min(MAX_BYTES)
		} else {
			MAX_BYTES
		};
		let downloaded = self
			.lifecycle()
			.artifacts
			.fetch(url, &mut file, limit, progress)
			.await?;

		let mismatch = |why: &str| refused(ErrorCode::ArtifactDigestMismatch, why);

		if body.asset.size > 0 && downloaded.size != body.asset.size {
			return Err(mismatch(
				"The downloaded file is not the size GitHub reported.",
			));
		}

		match &body.install.expected_sha256 {
			Some(expected) if *expected != downloaded.sha256 => Err(mismatch(
				"The downloaded file does not match its expected checksum.",
			)),
			_ => Ok((downloaded.size, downloaded.sha256)),
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn file_names_cannot_escape_or_be_empty() {
		assert_eq!(
			safe_file_name("app-1.0.tar.gz").as_deref(),
			Some("app-1.0.tar.gz")
		);
		assert_eq!(
			safe_file_name("../../etc/passwd").as_deref(),
			Some("_.._etc_passwd")
		);
		assert_eq!(safe_file_name("a/b").as_deref(), Some("a_b"));
		assert_eq!(safe_file_name("..").as_deref(), None);
		assert_eq!(safe_file_name("   ").as_deref(), None);
		assert_eq!(safe_file_name(&"x".repeat(300)), None);
		assert!(!safe_file_name("..\\evil").unwrap().contains(['/', '\\']));
	}
}
