//! Application updates: what is available, whether it may be applied
//! automatically, and the cycle that applies it.
//!
//! These settings are preferences, never trust roots. They can delay or
//! pause an update. Nothing here can authorize one that verification would
//! refuse.

mod cycle;
mod decide;
mod launch;
mod outlook;
mod settings;
mod status;

pub use cycle::{CycleOutcome, CycleReport, CycleResult, Trigger};
pub use decide::{Decision, Initiator, SkipReason, decide, effective_age};
pub use launch::LaunchOutcome;
pub use outlook::Outlook;
pub use settings::{ModeConsent, UpdateSettings};
pub use status::{AutoStatus, AvailableUpdate, OperatorUpdate, UpdateState, UpdateStatus};
