//! In-memory buffer of recent share attempts. Used by StubShareProcessor and API.

use crate::{ShareResult, ShareSubmission};
use std::collections::VecDeque;
use tokio::sync::RwLock;

const MAX_RECENT: usize = 100;

/// In-memory buffer of recent (share, result) pairs for API exposure.
#[derive(Default)]
pub struct RecentSharesBuffer {
    entries: RwLock<VecDeque<ShareAttempt>>,
}

#[derive(Clone, serde::Serialize)]
pub struct ShareAttempt {
    pub worker_id: String,
    pub job_id: String,
    pub accepted: bool,
    pub reject_reason: Option<String>,
}

impl RecentSharesBuffer {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn record(&self, share: &ShareSubmission, result: &ShareResult) {
        let attempt = ShareAttempt {
            worker_id: share.worker.id.clone(),
            job_id: share.job_id.clone(),
            accepted: result.is_accepted(),
            reject_reason: result.reject_reason(),
        };
        let mut entries = self.entries.write().await;
        if entries.len() >= MAX_RECENT {
            entries.pop_front();
        }
        entries.push_back(attempt);
    }

    pub async fn recent(&self) -> Vec<ShareAttempt> {
        let entries = self.entries.read().await;
        entries.iter().cloned().collect()
    }
}
