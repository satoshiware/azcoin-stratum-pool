//! Per-connection state for SV1 sessions.

use crate::messages::Sv1VersionRollingConfig;
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
    /// Negotiated version rolling settings from mining.configure.
    pub version_rolling: Option<Sv1VersionRollingConfig>,
    /// Last job_id sent via mining.notify, used to deduplicate push notifies.
    pub last_notify_job_id: Option<String>,
}
