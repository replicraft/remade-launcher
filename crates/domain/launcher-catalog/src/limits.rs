//! Resource limits applied before and during parsing.

/// Largest manifest accepted from the network or the cache.
pub const MAX_MANIFEST_BYTES: usize = 16 * 1024 * 1024;

/// Largest signature envelope.
pub const MAX_ENVELOPE_BYTES: usize = 64 * 1024;

/// Deepest JSON nesting the strict parser allows.
pub const MAX_JSON_DEPTH: usize = 32;

/// The largest integer every JSON implementation represents exactly.
pub const MAX_SAFE_INTEGER: u64 = 9_007_199_254_740_991;

pub const MAX_SIGNATURES: usize = 16;
pub const MAX_PROJECTS: usize = 10_000;
pub const MAX_VARIANTS: usize = 128;

/// Longest allowed gap between issuance and expiry.
pub const MAX_LIFETIME_DAYS: i64 = 90;

/// How far in the future an issuance time may be, to tolerate clock skew.
pub const MAX_CLOCK_SKEW_MINUTES: i64 = 10;
