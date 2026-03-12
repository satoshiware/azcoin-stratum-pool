//! Per-connection state for SV1 sessions.

use pool_core::WorkerIdentity;

/// Session state tracked per TCP connection.
#[derive(Default)]
pub struct SessionState {
    /// Worker identity after successful authorize.
    pub authorized_worker: Option<WorkerIdentity>,
    /// True after mining.subscribe.
    pub subscribed: bool,
    /// Extranonce1 from subscribe response (hex).
    pub extranonce1: String,
    /// Extranonce2 size from subscribe response.
    pub extranonce2_size: u32,
}
