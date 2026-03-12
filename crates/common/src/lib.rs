//! Shared types, error handling, config primitives, logging helpers.
//! No business logic—only cross-cutting concerns used across crates.

pub mod config;
pub mod error;
pub mod tracing;

pub use config::*;
pub use error::*;
pub use tracing::*;
