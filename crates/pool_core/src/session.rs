//! Miner session abstraction.

use crate::WorkerIdentity;
use serde::{Deserialize, Serialize};

/// Represents an active miner session. Protocol-agnostic.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MinerSession {
    pub session_id: String,
    pub worker: WorkerIdentity,
    /// Optional extraversion string from miner.
    pub extra_nonce: Option<String>,
    /// When the session was created.
    pub created_at: u64,
}
