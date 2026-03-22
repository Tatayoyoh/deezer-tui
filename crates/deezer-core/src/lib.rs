pub mod api;
pub mod config;
pub mod decrypt;
pub mod offline;
pub mod player;

pub use config::Config;

/// Type alias for an HTTP client suitable for CDN downloads.
pub type CdnClient = reqwest::Client;
