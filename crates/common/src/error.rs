//! Shared error types and conversions.

use thiserror::Error;

/// Pool-wide error type. Use `anyhow` for application-level error propagation.
#[derive(Error, Debug)]
pub enum PoolError {
    #[error("configuration error: {0}")]
    Config(String),

    #[error("protocol error: {0}")]
    Protocol(String),

    #[error("storage error: {0}")]
    Storage(String),

    #[error("daemon/RPC error: {0}")]
    Daemon(String),

    #[error("internal error: {0}")]
    Internal(String),
}
