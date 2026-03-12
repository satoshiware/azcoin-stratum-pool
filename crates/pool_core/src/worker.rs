//! Worker identity and related types.

use serde::{Deserialize, Serialize};

/// Uniquely identifies a miner/worker in the pool.
/// Format is protocol-dependent (e.g. SV1: "username.worker_name").
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct WorkerIdentity {
    /// Full worker identifier (e.g. "user.worker").
    pub id: String,
    /// Optional username part for accounting.
    pub username: Option<String>,
    /// Optional worker name part.
    pub worker_name: Option<String>,
}

impl WorkerIdentity {
    pub fn new(id: impl Into<String>) -> Self {
        let id = id.into();
        let (username, worker_name) = parse_worker_parts(&id);
        Self {
            id,
            username,
            worker_name,
        }
    }
}

fn parse_worker_parts(id: &str) -> (Option<String>, Option<String>) {
    if let Some((u, w)) = id.split_once('.') {
        (
            Some(u.to_string()),
            if w.is_empty() {
                None
            } else {
                Some(w.to_string())
            },
        )
    } else {
        (Some(id.to_string()), None)
    }
}
