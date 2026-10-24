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

			/// The last eight hex digits, for showing people and for
			/// matching what they type. UUIDv7 puts a timestamp first, so
			/// the leading digits are identical for IDs made close in time;
			/// the tail is the random part.
			pub fn short(&self) -> String {
				let simple = self.0.simple().to_string();

				simple[simple.len() - 8..].to_owned()
			}

			/// True if `text` names this ID: the whole ID, or at least six
			/// trailing hex digits of it. Hyphens and case are ignored.
			pub fn matches(&self, text: &str) -> bool {
				let wanted: String = text.chars().filter(|c| *c != '-').collect::<String>().to_lowercase();
				let simple = self.0.simple().to_string();

				wanted.len() >= 6 && simple.ends_with(&wanted)
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
	fn ids_made_together_still_have_distinct_short_forms() {
		let ids: Vec<_> = (0..50).map(|_| InstallationId::new()).collect();
		let shorts: std::collections::HashSet<_> = ids.iter().map(InstallationId::short).collect();

		assert_eq!(shorts.len(), ids.len());
	}

	#[test]
	fn matching_accepts_the_tail_or_the_whole_id_only() {
		let id = InstallationId::new();

		assert!(id.matches(&id.short()));
		assert!(id.matches(&id.to_string()));
		assert!(id.matches(&id.to_string().to_uppercase()));
		assert!(!id.matches("abc"), "too short to mean anything");
		assert!(!id.matches(&"0".repeat(8)) || id.short() == "00000000");
	}

	#[test]
	fn rejects_garbage() {
		assert!("not-an-id".parse::<InstallationId>().is_err());
	}
}
