//! Balance entry for worker accounting.

use crate::WorkerIdentity;
use serde::{Deserialize, Serialize};

/// A balance entry for a worker.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BalanceEntry {
    pub worker: WorkerIdentity,
    pub amount_sat: i64,
    pub updated_at: u64,
}
