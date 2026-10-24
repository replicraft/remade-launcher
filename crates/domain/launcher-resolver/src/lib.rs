//! Release resolution and installation plans.
//!
//! The resolver reads observations and signed policy and explains what it
//! found. It has no side effects. A [`Plan`] freezes one candidate into an
//! immutable, hashed record that a user approves, so what they approved is
//! exactly what later executes.

pub mod heuristics;
mod matching;
mod plan;
mod resolve;
mod version;

pub use heuristics::{CustomOption, Fit, Guess, classify_asset, custom_candidates};
pub use matching::Matcher;
pub use plan::{
	ApprovedPlan, AssetBinding, Consent, EvidenceLevel, InstallBinding, Operation, Origin, Plan,
	PlanBody, PlanError, ReleaseBinding, SourceBinding,
};
pub use resolve::{
	AssetRef, Candidate, NoCompatible, OrderBasis, Outcome, ReleaseRef, Resolution, SkippedRelease,
	resolve_curated, resolve_curated_on,
};
pub use version::{Version, VersionOrder};
