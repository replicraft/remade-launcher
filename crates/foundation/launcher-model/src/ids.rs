//! Opaque identifiers.
//!
//! Identifiers are UUIDv7 values, so they sort by creation time. That keeps
//! SQLite primary keys append-mostly, which is kind to B-tree pages.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

macro_rules! define_id {
	($(#[$meta:meta])* $name:ident) => {
		$(#[$meta])*
		#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
		#[serde(transparent)]
		pub struct $name(Uuid);

		impl $name {
			/// Generates a new identifier.
			pub fn new() -> Self {
				Self(Uuid::now_v7())
			}

			/// The identifier as raw bytes, suitable for a SQLite BLOB column.
			pub fn as_bytes(&self) -> &[u8; 16] {
				self.0.as_bytes()
			}

			/// Rebuilds an identifier from the bytes stored by [`Self::as_bytes`].
			pub fn from_bytes(bytes: [u8; 16]) -> Self {
				Self(Uuid::from_bytes(bytes))
			}
		}

		impl Default for $name {
			fn default() -> Self {
				Self::new()
			}
		}

		impl fmt::Display for $name {
			fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
				self.0.hyphenated().fmt(formatter)
			}
		}

		impl FromStr for $name {
			type Err = uuid::Error;

			fn from_str(text: &str) -> Result<Self, Self::Err> {
				Uuid::parse_str(text).map(Self)
			}
		}
	};
}

define_id! {
	/// One installed instance of an application.
	///
	/// An application can have several installations, which differ by
	/// architecture, channel, or scope.
	InstallationId
}

define_id! {
	/// One durable operation: an install, update, repair, or removal.
	OperationId
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn round_trips_through_text() {
		let id = OperationId::new();
		let parsed: OperationId = id.to_string().parse().unwrap();

		assert_eq!(id, parsed);
	}

	#[test]
	fn round_trips_through_bytes() {
		let id = InstallationId::new();

		assert_eq!(id, InstallationId::from_bytes(*id.as_bytes()));
	}

	#[test]
	fn later_identifiers_sort_after_earlier_ones() {
		let first = OperationId::new();
		std::thread::sleep(std::time::Duration::from_millis(2));
		let second = OperationId::new();

		assert!(first < second);
	}

	#[test]
	fn rejects_garbage() {
		assert!("not-an-id".parse::<InstallationId>().is_err());
	}
}
