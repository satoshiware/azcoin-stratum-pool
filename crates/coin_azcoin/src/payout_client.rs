//! Placeholder payout client abstraction. Not implemented yet.

use pool_core::{PayoutRecord, WorkerIdentity};

/// Payout client for AZCOIN. Placeholder—no implementation.
#[derive(Debug, Clone, Default)]
pub struct AzcoinPayoutClient;

impl AzcoinPayoutClient {
    pub fn new() -> Self {
        Self
    }

    /// Placeholder: execute payout to worker address.
    pub async fn execute(
        &self,
        _worker: &WorkerIdentity,
        _amount_sat: i64,
    ) -> Result<PayoutRecord, String> {
        Err("payout not implemented".to_string())
    }
}
