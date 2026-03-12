//! Payout record placeholder.

use crate::WorkerIdentity;
use serde::{Deserialize, Serialize};

/// A payout record. Placeholder for future implementation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PayoutRecord {
    pub payout_id: String,
    pub worker: WorkerIdentity,
    pub amount_sat: i64,
    pub txid: Option<String>,
    pub created_at: u64,
}
