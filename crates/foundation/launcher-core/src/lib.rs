//! The headless engine.
//!
//! Nothing here knows about windows. The interface submits typed commands
//! and listens for events, and the same engine can sit behind a CLI.

mod engine;
mod events;
mod paths;

pub use engine::Engine;
pub use events::EngineEvent;
pub use paths::DataDirectory;
