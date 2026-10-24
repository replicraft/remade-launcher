//! Shared vocabulary for the Remade Launcher.
//!
//! This crate has no windowing, networking, or storage dependencies. Every
//! other crate may depend on it, and it depends on none of them.

pub mod error;
pub mod ids;
pub mod local_time;
pub mod preferences;
pub mod product;
pub mod settings;
pub mod target;
pub mod updates;

pub use error::{ErrorCategory, ErrorCode, LauncherError, Result, Retry};
pub use ids::{InstallationId, OperationId};
pub use preferences::{
	Language, LauncherUpdates, Layout, Network, Notifications, Preferences, Proxy, StartPage,
};
pub use settings::ThemePreference;
pub use target::{Abi, Arch, HostTarget, Os};
pub use updates::{CONSENT_POLICY_VERSION, MinimumAge, UpdateMode};
