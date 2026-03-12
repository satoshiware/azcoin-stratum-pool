//! Database models. Aligned with pool_core domain types.

use pool_core::WorkerIdentity;
use serde::{Deserialize, Serialize};

/// DB representation of worker.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerModel {
    pub id: String,
    pub username: Option<String>,
    pub worker_name: Option<String>,
}

impl From<WorkerIdentity> for WorkerModel {
    fn from(w: WorkerIdentity) -> Self {
        Self {
            id: w.id,
            username: w.username,
            worker_name: w.worker_name,
        }
    }
}

impl From<WorkerModel> for WorkerIdentity {
    fn from(m: WorkerModel) -> Self {
        WorkerIdentity {
            id: m.id,
            username: m.username,
            worker_name: m.worker_name,
        }
    }
}

/// DB representation of share. Stub schema.
#[derive(Debug, Clone)]
pub struct ShareModel {
    pub id: i64,
    pub worker_id: String,
    pub job_id: String,
    pub result: String,
    pub created_at: i64,
}

/// DB representation of round. Stub schema.
#[derive(Debug, Clone)]
pub struct RoundModel {
    pub round_id: String,
    pub height: i64,
    pub prev_hash: Vec<u8>,
    pub started_at: i64,
    pub status: String,
}

/// DB representation of balance. Stub schema.
#[derive(Debug, Clone)]
pub struct BalanceModel {
    pub worker_id: String,
    pub amount_sat: i64,
    pub updated_at: i64,
}

/// DB representation of payout. Stub schema.
#[derive(Debug, Clone)]
pub struct PayoutModel {
    pub payout_id: String,
    pub worker_id: String,
    pub amount_sat: i64,
    pub txid: Option<String>,
    pub created_at: i64,
}
