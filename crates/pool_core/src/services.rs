//! Pool service stubs for the first vertical slice.
//! Worker registration, in-memory stats, placeholder job source.

use crate::{
    Job, JobSource, PoolStats, RecentSharesBuffer, ShareProcessor, ShareResult, ShareSubmission,
    WorkerIdentity,
};
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::RwLock;

/// In-memory worker registry. Stub for the first slice.
#[derive(Default)]
pub struct InMemoryWorkerRegistry {
    workers: RwLock<HashMap<String, WorkerIdentity>>,
}

impl InMemoryWorkerRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a worker. Idempotent—overwrites if already present.
    pub async fn register(&self, worker: WorkerIdentity) {
        let mut w = self.workers.write().await;
        w.insert(worker.id.clone(), worker);
    }

    /// List all registered workers.
    pub async fn list(&self) -> Vec<WorkerIdentity> {
        let w = self.workers.read().await;
        w.values().cloned().collect()
    }

    /// Count of registered workers.
    pub async fn count(&self) -> u64 {
        let w = self.workers.read().await;
        w.len() as u64
    }
}

/// In-memory stats snapshot. Updated on connect/disconnect and worker registration.
pub struct InMemoryStatsSnapshot {
    pool_name: RwLock<String>,
    active_connections: AtomicU64,
    worker_count: AtomicU64,
    round_height: AtomicU64,
    round_status: RwLock<String>,
}

impl InMemoryStatsSnapshot {
    pub fn new(pool_name: impl Into<String>) -> Self {
        Self {
            pool_name: RwLock::new(pool_name.into()),
            active_connections: AtomicU64::new(0),
            worker_count: AtomicU64::new(0),
            round_height: AtomicU64::new(0),
            round_status: RwLock::new("open".to_string()),
        }
    }

    pub fn record_connection(&self) {
        self.active_connections.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_disconnection(&self) {
        self.active_connections.fetch_sub(1, Ordering::Relaxed);
    }

    pub fn set_worker_count(&self, n: u64) {
        self.worker_count.store(n, Ordering::Relaxed);
    }

    pub fn set_round_height(&self, h: u64) {
        self.round_height.store(h, Ordering::Relaxed);
    }

    /// Take a snapshot of current stats. Safe to call from async.
    pub async fn snapshot(&self) -> PoolStats {
        let pool_name = self.pool_name.read().await.clone();
        let round_status = self.round_status.read().await.clone();
        PoolStats {
            pool_name,
            hashrate: 0.0,
            worker_count: self.worker_count.load(Ordering::Relaxed),
            round_height: self.round_height.load(Ordering::Relaxed),
            round_status,
        }
    }
}

/// Placeholder job source. Returns a single stub job for notify skeleton.
#[derive(Default)]
pub struct StubJobSource;

#[async_trait]
impl JobSource for StubJobSource {
    async fn current_job(&self) -> Option<Job> {
        Some(Job::placeholder())
    }
}

/// Stub share processor. Rejects all shares with a clear reason.
pub struct StubShareProcessor {
    recent_buffer: Arc<RecentSharesBuffer>,
}

impl StubShareProcessor {
    pub fn new(recent_buffer: Arc<RecentSharesBuffer>) -> Self {
        Self { recent_buffer }
    }
}

#[async_trait]
impl ShareProcessor for StubShareProcessor {
    async fn process_share(&self, share: ShareSubmission) -> ShareResult {
        let result = ShareResult::Rejected {
            reason: "share validation not implemented".to_string(),
        };
        self.recent_buffer.record(&share, &result).await;
        result
    }
}

/// Bundles pool services for wiring. Used by main binary and API server.
pub struct PoolServices {
    pub worker_registry: Arc<InMemoryWorkerRegistry>,
    pub stats: Arc<InMemoryStatsSnapshot>,
    pub job_source: Arc<dyn JobSource>,
    pub share_processor: Arc<dyn ShareProcessor>,
    pub recent_shares: Arc<RecentSharesBuffer>,
}

impl PoolServices {
    /// Create pool services with a custom job source (e.g. daemon-backed).
    pub fn new(pool_name: impl Into<String>, job_source: Arc<dyn JobSource>) -> Self {
        let recent_shares = Arc::new(RecentSharesBuffer::new());
        let share_processor: Arc<dyn ShareProcessor> =
            Arc::new(StubShareProcessor::new(Arc::clone(&recent_shares)));
        Self {
            worker_registry: Arc::new(InMemoryWorkerRegistry::new()),
            stats: Arc::new(InMemoryStatsSnapshot::new(pool_name)),
            job_source,
            share_processor,
            recent_shares,
        }
    }

    /// Create pool services with stub job source (placeholder jobs only).
    pub fn with_stub_job_source(pool_name: impl Into<String>) -> Self {
        Self::new(pool_name, Arc::new(StubJobSource))
    }
}
