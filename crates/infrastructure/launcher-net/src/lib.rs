//! The launcher's HTTP client.
//!
//! One [`HttpClient`] is built at startup and shared. It reuses connections
//! (HTTP/2 where offered), decompresses responses, and refuses anything that
//! is not HTTPS. Every download is bounded: a hostile or broken server cannot
//! make the launcher buffer an unlimited response.

mod bounded;
mod client;

pub use client::HttpClient;
pub use reqwest::Url;
