//! Release providers.
//!
//! A provider reports a repository's identity and its published releases. It
//! has no say in what Remade approves, and nothing it returns is trusted to
//! pick an executable: that is the resolver's and the user's job.

mod cache;
mod github;
mod model;
mod source_url;
mod transport;

pub use cache::{CachedTransport, ResponseStore, StoredResponse};
pub use github::GithubProvider;
pub use model::{Asset, Release, ReleaseObservation, RepositoryIdentity};
pub use source_url::{SourceRef, parse_source_url};
pub use transport::Transport;
