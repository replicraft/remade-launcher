//! The background updater's machinery.
//!
//! The updater is a separate per-user program. It runs with ordinary user
//! privileges, starts when the user signs in (and only if they asked for
//! that), checks for updates on a 24-hour schedule, and answers a few
//! requests over a local socket that only that user can reach.
//!
//! This crate holds what the CLI, the desktop app, and the updater itself
//! share: registering and unregistering the login item on each operating
//! system, the local protocol, and the scheduling rules.

pub mod ipc;
pub mod paths;
pub mod registration;
pub mod schedule;
pub mod service;

pub use paths::BackgroundPaths;
pub use registration::{LaunchSpec, Registrar, RegistrationState, host_registrar};
pub use service::{Backend, BoxFuture, Clock, CycleSummary, Service, SystemClock};

/// The login item's label on every system that has one.
pub const SERVICE_LABEL: &str = "wiki.remade.Launcher.Updater";

/// The updater executable's file name, without a Windows extension.
pub const DAEMON_BINARY: &str = "remade-launcher-daemon";
