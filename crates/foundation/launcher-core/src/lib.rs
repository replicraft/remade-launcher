//! The headless engine.
//!
//! Nothing here knows about windows. The interface submits typed commands
//! and listens for events, and the same engine can sit behind a CLI.

mod catalog;
mod config;
mod engine;
mod events;
mod explore;
mod lifecycle;
mod paths;
mod prepare;

#[cfg(test)]
mod flow_tests;

pub use catalog::{CatalogService, CatalogState, Origin, Problem, ProblemKind};
pub use config::{CatalogConfig, EngineConfig};
pub use engine::Engine;
pub use events::{EngineEvent, OperationEvent, Stage};
pub use explore::{Availability, ExploreQuery, ExploreRow};
pub use launcher_catalog::{
	Freshness, PackageFormat, Project, State, VerificationLevel, VerifiedCatalog,
};
pub use lifecycle::{
	ArtifactSource, HttpArtifacts, InstallMeta, InstalledApp, LifecycleService, RecoveryReport,
	RemovalReport,
};
pub use paths::DataDirectory;
pub use prepare::{CustomPreparation, Preparation, Ready, custom_plan};
