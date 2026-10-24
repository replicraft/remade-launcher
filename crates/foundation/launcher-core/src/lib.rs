//! The headless engine.
//!
//! Nothing here knows about windows. The interface submits typed commands
//! and listens for events, and the same engine can sit behind a CLI.

mod adopt;
mod browse;
mod catalog;
mod config;
mod credentials;
mod diagnostics;
mod download;
mod engine;
mod events;
mod explore;
mod inspect;
mod lifecycle;
mod network;
mod paths;
mod prepare;
mod repair;
mod saved_plans;
mod storage_report;
mod updates;

#[cfg(test)]
mod flow_tests;

pub use adopt::{AdoptedApp, AdoptionCandidate, AdoptionRequest};
pub use browse::{Changelog, PlatformReport, TrustStatus, VariantSupport};
pub use catalog::{CatalogService, CatalogState, Origin, Problem, ProblemKind};
pub use config::{CatalogConfig, EngineConfig};
pub use credentials::{Keychain, MemorySecret, SecretError, SecretStore, TokenState};
pub use download::DownloadedFile;
pub use engine::Engine;
pub use events::{EngineEvent, OperationEvent, Stage};
pub use explore::{Availability, ExploreQuery, ExploreRow};
pub use inspect::{CacheStatus, DoctorReport, VerifyReport};
pub use launcher_catalog::{
	Freshness, Maturity, PackageFormat, Project, State, VerificationLevel, VerifiedCatalog,
};
pub use launcher_resolver::{ReleaseStatus, ReleaseView};
pub use lifecycle::{
	ArtifactSource, HttpArtifacts, InstallMeta, InstalledApp, LifecycleService, OperationSummary,
	RecoveryReport, RemovalReport,
};
pub use network::{RateLimit, StoredResponses, TOKEN_VARIABLE};
pub use paths::DataDirectory;
pub use prepare::{CustomPreparation, Preparation, Ready, custom_plan};
pub use repair::{RepairAssessment, RepairOutcome};
pub use saved_plans::{Applied, Approval, SavedPlan, UninstallPlan};
pub use storage_report::StorageReport;
pub use updates::{
	AutoStatus, AvailableUpdate, CycleOutcome, CycleReport, CycleResult, Decision, Initiator,
	LaunchOutcome, ModeConsent, OperatorUpdate, Outlook, SkipReason, Trigger, UpdateSettings,
	UpdateState, UpdateStatus, decide, effective_age,
};
