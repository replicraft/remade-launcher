//! Versioned schema migrations.
//!
//! The applied version lives in SQLite's `user_version`. Migrations are
//! append-only: never edit one that has shipped, add a new one.

use launcher_model::{ErrorCode, LauncherError};
use rusqlite::{Connection, TransactionBehavior};

use crate::error::unavailable;

struct Migration {
	version: u32,
	sql: &'static str,
}

const MIGRATIONS: &[Migration] = &[
	Migration {
		version: 1,
		sql: include_str!("migrations/0001_settings.sql"),
	},
	Migration {
		version: 2,
		sql: include_str!("migrations/0002_catalog_trust.sql"),
	},
	Migration {
		version: 3,
		sql: include_str!("migrations/0003_lifecycle.sql"),
	},
	Migration {
		version: 4,
		sql: include_str!("migrations/0004_plans_and_consents.sql"),
	},
	Migration {
		version: 5,
		sql: include_str!("migrations/0005_update_policy.sql"),
	},
	Migration {
		version: 6,
		sql: include_str!("migrations/0006_http_cache.sql"),
	},
	Migration {
		version: 7,
		sql: include_str!("migrations/0007_adopted_apps.sql"),
	},
	Migration {
		version: 8,
		sql: include_str!("migrations/0008_favorites.sql"),
	},
	Migration {
		version: 9,
		sql: include_str!("migrations/0009_app_update_mode.sql"),
	},
];

/// The schema version this build understands.
pub(crate) fn latest_version() -> u32 {
	MIGRATIONS.last().map_or(0, |migration| migration.version)
}

/// Brings the database up to [`latest_version`].
///
/// Refuses to touch a database written by a newer launcher, so that a
/// downgrade cannot corrupt data it does not understand.
pub(crate) fn run(connection: &mut Connection) -> Result<(), LauncherError> {
	let current: u32 = connection
		.pragma_query_value(None, "user_version", |row| row.get(0))
		.map_err(unavailable)?;

	if current > latest_version() {
		return Err(LauncherError::new(
			ErrorCode::StorageNewerThanApp,
			"The launcher's data was written by a newer version. Update the launcher to continue.",
		));
	}

	for migration in MIGRATIONS
		.iter()
		.filter(|migration| migration.version > current)
	{
		apply(connection, migration)?;
		tracing::info!(version = migration.version, "applied database migration");
	}

	Ok(())
}

/// Applies one migration and its version bump in a single transaction.
fn apply(connection: &mut Connection, migration: &Migration) -> Result<(), LauncherError> {
	let failed = |error: rusqlite::Error| {
		LauncherError::new(
			ErrorCode::StorageMigrationFailed,
			"The launcher's database could not be upgraded.",
		)
		.with_source(error)
	};

	let transaction = connection
		.transaction_with_behavior(TransactionBehavior::Immediate)
		.map_err(failed)?;

	transaction.execute_batch(migration.sql).map_err(failed)?;
	transaction
		.pragma_update(None, "user_version", migration.version)
		.map_err(failed)?;

	transaction.commit().map_err(failed)
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn migrates_a_fresh_database() {
		let mut connection = Connection::open_in_memory().unwrap();

		run(&mut connection).unwrap();

		let version: u32 = connection
			.pragma_query_value(None, "user_version", |row| row.get(0))
			.unwrap();
		assert_eq!(version, latest_version());
	}

	#[test]
	fn running_twice_changes_nothing() {
		let mut connection = Connection::open_in_memory().unwrap();

		run(&mut connection).unwrap();
		run(&mut connection).unwrap();
	}

	#[test]
	fn refuses_a_newer_database() {
		let mut connection = Connection::open_in_memory().unwrap();
		connection
			.pragma_update(None, "user_version", latest_version() + 1)
			.unwrap();

		let error = run(&mut connection).unwrap_err();

		assert_eq!(error.code(), ErrorCode::StorageNewerThanApp);
	}

	#[test]
	fn versions_are_sequential() {
		for (index, migration) in MIGRATIONS.iter().enumerate() {
			assert_eq!(migration.version as usize, index + 1);
		}
	}
}
