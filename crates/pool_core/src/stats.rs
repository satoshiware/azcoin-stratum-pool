//! Pool statistics abstraction.

use serde::{Deserialize, Serialize};

/// Pool-level statistics. Stub for API responses.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PoolStats {
    pub pool_name: String,
    pub hashrate: f64,
    pub worker_count: u64,
    pub round_height: u64,
    pub round_status: String,
}
