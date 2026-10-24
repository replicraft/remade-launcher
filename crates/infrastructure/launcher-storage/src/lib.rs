//! SQLite persistence for the launcher.
//!
//! [`Database`] owns one writer connection and a few read-only connections.
//! Each connection lives on its own thread, managed by `tokio-rusqlite`, so
//! no database call ever blocks an async task or the render loop. WAL mode
//! lets the readers run while the writer commits.

mod database;
mod error;
mod migrations;
mod pragmas;
mod settings;

pub use database::Database;
