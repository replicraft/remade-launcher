//! SQLite persistence for the launcher.
//!
//! [`Database`] owns one writer connection and a few read-only connections.
//! Each connection lives on its own thread, managed by `tokio-rusqlite`, so
//! no database call ever blocks an async task or the render loop. WAL mode
//! lets the readers run while the writer commits.

mod catalog_trust;
mod database;
mod error;
mod lifecycle;
mod migrations;
mod plans;
mod policy;
mod pragmas;
mod settings;

pub use catalog_trust::StoredCatalog;
pub use database::Database;
pub use lifecycle::{InstallationRecord, OperationRecord};
pub use plans::{ConsentRecord, StoredPlan};
pub use policy::{AppPolicy, AssetObservation, DecisionRecord, ObservedAsset};
