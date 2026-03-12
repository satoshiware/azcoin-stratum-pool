//! Share submission and result types.

use crate::WorkerIdentity;
use serde::{Deserialize, Serialize};

/// A share submitted by a miner.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShareSubmission {
    pub job_id: String,
    pub worker: WorkerIdentity,
    pub extra_nonce2: Vec<u8>,
    pub ntime: u32,
    pub nonce: u32,
}

/// Result of share validation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ShareResult {
    /// Share meets pool difficulty.
    Accepted,
    /// Share meets block difficulty (potential block).
    Block,
    /// Share rejected (duplicate, stale, invalid).
    Rejected { reason: String },
}
