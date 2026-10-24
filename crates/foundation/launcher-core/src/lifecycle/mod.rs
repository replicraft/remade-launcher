//! The install, update, launch, and uninstall lifecycle.
//!
//! Every destructive step is journaled in SQLite before it happens, and the
//! journal, not memory, decides what recovery does after a crash. Nothing
//! here executes downloaded code except [`LifecycleService::launch`], which
//! runs the entry point the user asked to open.

mod artifact;
mod install;
mod launch;
mod lock;
mod meta;
mod quarantine;
mod recover;
mod remove;
mod safe_delete;

pub use artifact::{ArtifactSource, HttpArtifacts};
pub use install::LifecycleService;
pub(crate) use lock::acquire_patiently as acquire_operation_lock;
pub(crate) use lock::try_acquire_cycle as cycle_lock;
pub(crate) use meta::ManifestEntry;
pub use meta::{InstallMeta, InstalledApp, OperationSummary};
pub use recover::RecoveryReport;
pub use remove::RemovalReport;
pub(crate) use remove::verify_ownership;

#[cfg(test)]
mod tests;
