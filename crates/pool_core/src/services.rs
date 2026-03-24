//! Pool service stubs for the first vertical slice.
//! Worker registration, in-memory stats, placeholder job source.

use crate::{
    Job, JobSource, PoolStats, RecentSharesBuffer, ShareProcessor, ShareResult, ShareSubmission,
    ShareValidator, WorkerIdentity,
};
use async_trait::async_trait;
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::RwLock;

const ACTIVE_JOBS_MAX: usize = 64;

/// In-memory registry of recently issued jobs. Bounded by ACTIVE_JOBS_MAX.
/// Jobs are registered when mining.notify is sent. Share validation checks against this.
#[derive(Default)]
pub struct ActiveJobRegistry {
    inner: RwLock<ActiveJobRegistryInner>,
}

#[derive(Default)]
struct ActiveJobRegistryInner {
    by_id: HashMap<String, Job>,
    order: VecDeque<String>,
}

impl ActiveJobRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a job. Called when mining.notify is sent.
    pub async fn register(&self, job: Job) {
        let mut inner = self.inner.write().await;
        if inner.by_id.contains_key(&job.job_id) {
            return;
        }
        while inner.order.len() >= ACTIVE_JOBS_MAX {
            if let Some(old_id) = inner.order.pop_front() {
                inner.by_id.remove(&old_id);
            }
        }
        inner.order.push_back(job.job_id.clone());
        inner.by_id.insert(job.job_id.clone(), job);
    }

    /// Check if job_id is in the active/recent registry.
    pub async fn contains(&self, job_id: &str) -> bool {
        let inner = self.inner.read().await;
        inner.by_id.contains_key(job_id)
    }

    /// Get job by id. Used for cryptographic validation.
    pub async fn get_job(&self, job_id: &str) -> Option<Job> {
        let inner = self.inner.read().await;
        inner.by_id.get(job_id).cloned()
    }
}

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

/// Placeholder job source. Returns a single placeholder job for notify skeleton.
#[derive(Default)]
pub struct StubJobSource;

#[async_trait]
impl JobSource for StubJobSource {
    async fn current_job(&self) -> Option<Job> {
        Some(Job::placeholder())
    }
}

/// Job source that returns no job. For testing no-job behavior.
#[derive(Default)]
pub struct NoJobSource;

#[async_trait]
impl JobSource for NoJobSource {
    async fn current_job(&self) -> Option<Job> {
        None
    }
}

/// Job source that returns a fixed job. For testing live notify.
#[derive(Clone)]
pub struct FixedJobSource {
    job: Job,
}

impl FixedJobSource {
    pub fn new(job: Job) -> Self {
        Self { job }
    }
}

#[async_trait]
impl JobSource for FixedJobSource {
    async fn current_job(&self) -> Option<Job> {
        Some(self.job.clone())
    }
}

/// Job source that returns jobs from a sequence. For testing prior-job acceptance.
/// Each call to current_job() returns the next job in the list (wraps around).
pub struct VecJobSource {
    jobs: Vec<Job>,
    next: std::sync::atomic::AtomicUsize,
}

impl VecJobSource {
    pub fn new(jobs: Vec<Job>) -> Self {
        Self {
            jobs,
            next: std::sync::atomic::AtomicUsize::new(0),
        }
    }
}

#[async_trait]
impl JobSource for VecJobSource {
    async fn current_job(&self) -> Option<Job> {
        if self.jobs.is_empty() {
            return None;
        }
        let idx = self.next.fetch_add(1, std::sync::atomic::Ordering::SeqCst) % self.jobs.len();
        Some(self.jobs[idx].clone())
    }
}

/// Test validator that always accepts. Used when crypto validation is not under test.
pub struct AcceptAllShareValidator;

impl ShareValidator for AcceptAllShareValidator {
    fn validate_share(
        &self,
        _job: &Job,
        _share: &ShareSubmission,
        _extranonce1: &[u8],
        _pool_difficulty: u32,
    ) -> ShareResult {
        ShareResult::Accepted
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

/// Share processor that validates job linkage and optionally runs coin-specific crypto validation.
pub struct JobAwareShareProcessor {
    job_registry: Arc<ActiveJobRegistry>,
    share_validator: Option<Arc<dyn ShareValidator>>,
    pool_difficulty: u32,
    recent_buffer: Arc<RecentSharesBuffer>,
}

impl JobAwareShareProcessor {
    pub fn new(
        job_registry: Arc<ActiveJobRegistry>,
        recent_buffer: Arc<RecentSharesBuffer>,
    ) -> Self {
        Self {
            job_registry,
            share_validator: None,
            pool_difficulty: 4,
            recent_buffer,
        }
    }

    pub fn with_validator(
        job_registry: Arc<ActiveJobRegistry>,
        share_validator: Arc<dyn ShareValidator>,
        pool_difficulty: u32,
        recent_buffer: Arc<RecentSharesBuffer>,
    ) -> Self {
        Self {
            job_registry,
            share_validator: Some(share_validator),
            pool_difficulty,
            recent_buffer,
        }
    }
}

#[async_trait]
impl ShareProcessor for JobAwareShareProcessor {
    async fn process_share(&self, share: ShareSubmission) -> ShareResult {
        if let Some(ref ctx) = share.validation_context {
            if let Some(expected_len) = ctx.expected_extra_nonce2_len {
                if share.extra_nonce2.len() != expected_len {
                    let result = ShareResult::Rejected {
                        reason: format!(
                            "extra_nonce2 must be {} bytes, got {}",
                            expected_len,
                            share.extra_nonce2.len()
                        ),
                    };
                    self.recent_buffer.record(&share, &result).await;
                    return result;
                }
            }
            if let Some(version_bits) = ctx.version_bits {
                match ctx.version_rolling_mask {
                    Some(mask) => {
                        if version_bits & !mask != 0 {
                            let result = ShareResult::Rejected {
                                reason: format!(
                                    "version_bits {:08x} outside negotiated mask {:08x}",
                                    version_bits, mask
                                ),
                            };
                            self.recent_buffer.record(&share, &result).await;
                            return result;
                        }
                    }
                    None => {
                        let result = ShareResult::Rejected {
                            reason: "version rolling not negotiated".to_string(),
                        };
                        self.recent_buffer.record(&share, &result).await;
                        return result;
                    }
                }
            }
        }

        let job = match self.job_registry.get_job(&share.job_id).await {
            Some(j) => j,
            None => {
                let result = ShareResult::UnknownJob {
                    reason: format!("unknown job_id {}", share.job_id),
                };
                self.recent_buffer.record(&share, &result).await;
                return result;
            }
        };

        if let Some(ref validator) = self.share_validator {
            let extranonce1 = match &share.validation_context {
                Some(ctx) => match &ctx.extranonce1_hex {
                    Some(hex) => match hex::decode(hex) {
                        Ok(b) => b,
                        Err(_) => {
                            let result = ShareResult::Malformed {
                                reason: "extranonce1 invalid hex".to_string(),
                            };
                            self.recent_buffer.record(&share, &result).await;
                            return result;
                        }
                    },
                    None => vec![0u8; 4],
                },
                None => vec![0u8; 4],
            };
            let result = validator.validate_share(&job, &share, &extranonce1, self.pool_difficulty);
            self.recent_buffer.record(&share, &result).await;
            result
        } else {
            let result = ShareResult::Accepted;
            self.recent_buffer.record(&share, &result).await;
            result
        }
    }
}

/// Bundles pool services for wiring. Used by main binary and API server.
pub struct PoolServices {
    pub worker_registry: Arc<InMemoryWorkerRegistry>,
    pub stats: Arc<InMemoryStatsSnapshot>,
    pub job_source: Arc<dyn JobSource>,
    pub job_registry: Arc<ActiveJobRegistry>,
    pub share_processor: Arc<dyn ShareProcessor>,
    pub recent_shares: Arc<RecentSharesBuffer>,
}

impl PoolServices {
    /// Create pool services with a custom job source (e.g. daemon-backed).
    /// Without validator: accepts shares that pass shape and job_id checks.
    pub fn new(pool_name: impl Into<String>, job_source: Arc<dyn JobSource>) -> Self {
        Self::new_inner(pool_name, job_source, None, 4)
    }

    /// Create pool services with coin-specific crypto validation.
    pub fn new_with_validator(
        pool_name: impl Into<String>,
        job_source: Arc<dyn JobSource>,
        share_validator: Arc<dyn ShareValidator>,
        pool_difficulty: u32,
    ) -> Self {
        Self::new_inner(
            pool_name,
            job_source,
            Some(share_validator),
            pool_difficulty,
        )
    }

    fn new_inner(
        pool_name: impl Into<String>,
        job_source: Arc<dyn JobSource>,
        share_validator: Option<Arc<dyn ShareValidator>>,
        pool_difficulty: u32,
    ) -> Self {
        let recent_shares = Arc::new(RecentSharesBuffer::new());
        let job_registry = Arc::new(ActiveJobRegistry::new());
        let share_processor: Arc<dyn ShareProcessor> = if let Some(validator) = share_validator {
            Arc::new(JobAwareShareProcessor::with_validator(
                Arc::clone(&job_registry),
                validator,
                pool_difficulty,
                Arc::clone(&recent_shares),
            ))
        } else {
            Arc::new(JobAwareShareProcessor::new(
                Arc::clone(&job_registry),
                Arc::clone(&recent_shares),
            ))
        };
        Self {
            worker_registry: Arc::new(InMemoryWorkerRegistry::new()),
            stats: Arc::new(InMemoryStatsSnapshot::new(pool_name)),
            job_source,
            job_registry,
            share_processor,
            recent_shares,
        }
    }

    /// Create pool services with placeholder job source (placeholder jobs only).
    pub fn with_placeholder_job_source(pool_name: impl Into<String>) -> Self {
        Self::new(pool_name, Arc::new(StubJobSource))
    }

    /// Create pool services with no job source (returns None). For testing no-job behavior.
    pub fn with_no_job_source(pool_name: impl Into<String>) -> Self {
        Self::new(pool_name, Arc::new(NoJobSource))
    }
}
