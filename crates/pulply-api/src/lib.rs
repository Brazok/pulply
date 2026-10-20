//! Pulp 3 REST API client used by the pulply TUI.

pub mod client;
pub mod error;
pub mod models;
pub mod status;

pub use client::{ClientConfig, PulpClient, VerifySsl};
pub use error::{Error, Result};
pub use models::{Page, ResourceExt};
pub use reqwest::Method;
pub use status::{Status, Version};
