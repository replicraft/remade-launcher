//! The headless engine.
//!
//! Nothing here knows about windows. The interface submits typed commands
//! and listens for events, and the same engine can sit behind a CLI.

mod catalog;
mod config;
mod engine;
mod events;
mod explore;
mod paths;

pub use catalog::{CatalogService, CatalogState, Origin, Problem, ProblemKind};
pub use config::{CatalogConfig, EngineConfig};
pub use engine::Engine;
pub use events::EngineEvent;
pub use explore::{Availability, ExploreQuery, ExploreRow};
pub use paths::DataDirectory;
