//! Product identity.
//!
//! These are selected defaults that the maintainer must freeze before the
//! first public release. They are never derived from a display name.

/// Display name shown in window titles and installers.
pub const NAME: &str = "Remade Launcher";

/// Tagline used on the about screen and in package descriptions.
pub const TAGLINE: &str = "The official launcher for unofficial apps.";

/// Reverse-DNS application identity. The desktop entry, hicolor icon name,
/// Flatpak ID, and macOS bundle identifier all use this value.
pub const APP_ID: &str = "wiki.remade.Launcher";

/// Identity of the signed catalog feed. Kept distinct from [`APP_ID`] so
/// catalog authority is never confused with the executable's identity.
pub const CATALOG_FEED_ID: &str = "wiki.remade.launcher-catalog";

/// Identity of the signed feed that announces launcher releases. It has its
/// own keys, apart from the catalog's.
pub const RELEASE_FEED_ID: &str = "wiki.remade.launcher-releases";

/// Version of this build, taken from the workspace manifest.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// `User-Agent` sent on every outgoing request.
pub fn user_agent() -> String {
	format!("RemadeLauncher/{VERSION} (+https://remade.wiki)")
}
