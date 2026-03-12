//! Round abstraction for block-finding cycles.

use serde::{Deserialize, Serialize};

/// A mining round (block-finding cycle).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Round {
    pub round_id: String,
    pub height: u64,
    pub prev_hash: [u8; 32],
    pub started_at: u64,
    pub status: RoundStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RoundStatus {
    Open,
    Closed,
    Paid,
}
