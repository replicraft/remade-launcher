//! Where the GitHub token lives.
//!
//! A token never goes in the launcher's database or its settings. It is kept
//! in the operating system's own store: Keychain on macOS, Credential Manager
//! on Windows, the Secret Service (GNOME Keyring, KWallet) on Linux and
//! FreeBSD. If there is no such store the feature is unavailable. It does not
//! fall back to a file.

use std::sync::{Arc, Mutex};

const SERVICE: &str = "wiki.remade.Launcher";
const ACCOUNT: &str = "github-token";

/// Why a secret could not be read or saved.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SecretError(pub String);

impl std::fmt::Display for SecretError {
	fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		formatter.write_str(&self.0)
	}
}

/// A place to keep one secret.
pub trait SecretStore: Send + Sync {
	fn get(&self) -> Result<Option<String>, SecretError>;
	fn set(&self, secret: &str) -> Result<(), SecretError>;
	fn clear(&self) -> Result<(), SecretError>;
}

/// The operating system's credential store.
pub struct Keychain;

#[cfg(any(
	target_os = "macos",
	target_os = "windows",
	target_os = "linux",
	target_os = "freebsd"
))]
impl Keychain {
	fn entry() -> Result<keyring::Entry, SecretError> {
		keyring::Entry::new(SERVICE, ACCOUNT).map_err(|error| SecretError(error.to_string()))
	}
}

#[cfg(any(
	target_os = "macos",
	target_os = "windows",
	target_os = "linux",
	target_os = "freebsd"
))]
impl SecretStore for Keychain {
	fn get(&self) -> Result<Option<String>, SecretError> {
		match Self::entry()?.get_password() {
			Ok(secret) => Ok(Some(secret)),
			Err(keyring::Error::NoEntry) => Ok(None),
			Err(error) => Err(SecretError(error.to_string())),
		}
	}

	fn set(&self, secret: &str) -> Result<(), SecretError> {
		Self::entry()?
			.set_password(secret)
			.map_err(|error| SecretError(error.to_string()))
	}

	fn clear(&self) -> Result<(), SecretError> {
		match Self::entry()?.delete_credential() {
			Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
			Err(error) => Err(SecretError(error.to_string())),
		}
	}
}

#[cfg(not(any(
	target_os = "macos",
	target_os = "windows",
	target_os = "linux",
	target_os = "freebsd"
)))]
impl SecretStore for Keychain {
	fn get(&self) -> Result<Option<String>, SecretError> {
		Ok(None)
	}

	fn set(&self, _secret: &str) -> Result<(), SecretError> {
		Err(SecretError(
			"This system has no credential store the launcher can use.".into(),
		))
	}

	fn clear(&self) -> Result<(), SecretError> {
		Ok(())
	}
}

/// A store that keeps its secret in memory, for tests.
#[derive(Clone, Default)]
pub struct MemorySecret(Arc<Mutex<Option<String>>>);

impl SecretStore for MemorySecret {
	fn get(&self) -> Result<Option<String>, SecretError> {
		Ok(self
			.0
			.lock()
			.map_err(|_| SecretError("poisoned".into()))?
			.clone())
	}

	fn set(&self, secret: &str) -> Result<(), SecretError> {
		*self.0.lock().map_err(|_| SecretError("poisoned".into()))? = Some(secret.to_owned());

		Ok(())
	}

	fn clear(&self) -> Result<(), SecretError> {
		*self.0.lock().map_err(|_| SecretError("poisoned".into()))? = None;

		Ok(())
	}
}

/// What the interface needs to know about the token, never the token itself.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TokenState {
	/// None is set.
	Absent,
	/// One is saved in the system store.
	Stored,
	/// `REMADE_GITHUB_TOKEN` supplies one, and wins over the stored one.
	FromEnvironment,
	/// The system store could not be used. The reason is for people, not logs.
	Unavailable(String),
}

/// Cleans up what a person pasted: surrounding space and a stray newline.
pub fn clean_token(pasted: &str) -> String {
	pasted.trim().to_owned()
}

/// A token is a single run of letters, digits, `_` and `-`. Anything else is
/// a paste mistake, and is refused before it can be sent anywhere.
pub fn token_looks_valid(token: &str) -> bool {
	(20..=255).contains(&token.len())
		&& token
			.chars()
			.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn the_memory_store_round_trips() {
		let store = MemorySecret::default();

		assert_eq!(store.get().unwrap(), None);

		store.set("ghp_example").unwrap();

		assert_eq!(store.get().unwrap().as_deref(), Some("ghp_example"));

		store.clear().unwrap();

		assert_eq!(store.get().unwrap(), None);
	}

	#[test]
	fn pasted_tokens_are_cleaned_and_checked() {
		assert_eq!(
			clean_token("  ghp_abcdefghijklmnopqrstuvwxyz0123456789\n"),
			"ghp_abcdefghijklmnopqrstuvwxyz0123456789"
		);
		assert!(token_looks_valid(
			"ghp_abcdefghijklmnopqrstuvwxyz0123456789"
		));
		assert!(token_looks_valid(
			"github_pat_11ABCDEFG0abcdefghijkl_mnopqrstuvwxyz"
		));

		for bad in [
			"",
			"short",
			"has space in it 0123456789012345",
			"newline\n0123456789012345678901",
			"ünïcode0123456789012345678",
		] {
			assert!(!token_looks_valid(bad), "{bad:?}");
		}
	}
}
