//! Share submission and result types.

use crate::WorkerIdentity;
use serde::{Deserialize, Serialize};

/// Optional validation context for share shape and session extranonce.
/// ntime/nonce hex width is validated at parse time. When present, ShareProcessor
/// validates extra_nonce2 length before job_id linkage.
#[derive(Debug, Clone, Default)]
pub struct ShareValidationContext {
    /// Expected byte length of extra_nonce2 (from session extranonce2_size).
    pub expected_extra_nonce2_len: Option<usize>,
    /// Extranonce1 from session (hex). Needed for coinbase reconstruction.
    pub extranonce1_hex: Option<String>,
    /// Negotiated version rolling mask for this session.
    pub version_rolling_mask: Option<u32>,
    /// Miner-selected version bits from mining.submit's optional sixth param.
    pub version_bits: Option<u32>,
}

/// A share submitted by a miner.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShareSubmission {
    pub job_id: String,
    pub worker: WorkerIdentity,
    pub extra_nonce2: Vec<u8>,
    pub ntime: u32,
    pub nonce: u32,
    /// When present, ShareProcessor validates shape before job_id linkage.
    #[serde(skip)]
    pub validation_context: Option<ShareValidationContext>,
}

/// Result of share validation. Distinguishes acceptance, low difficulty, block candidate,
/// malformed reconstruction, and unknown job.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ShareResult {
    /// Share meets pool difficulty.
    Accepted,
    /// Share meets block difficulty (potential block).
    Block,
    /// Structurally valid but hash below pool target.
    LowDifficulty { reason: String },
    /// Reconstruction or validation failed (malformed inputs).
    Malformed { reason: String },
    /// Unknown job_id (rejected before hashing).
    UnknownJob { reason: String },
    /// Other rejection (e.g. shape validation).
    Rejected { reason: String },
}

impl ShareResult {
    /// True if share is accepted or block candidate.
    pub fn is_accepted(&self) -> bool {
        matches!(self, ShareResult::Accepted | ShareResult::Block)
    }

    /// Human-readable reason for rejection. None for Accepted/Block.
    pub fn reject_reason(&self) -> Option<String> {
        match self {
            ShareResult::Accepted | ShareResult::Block => None,
            ShareResult::LowDifficulty { reason }
            | ShareResult::Malformed { reason }
            | ShareResult::UnknownJob { reason }
            | ShareResult::Rejected { reason } => Some(reason.clone()),
        }
    }
}
