//! API route handlers. Wired to pool services for stats and workers.

use axum::{extract::State, routing::get, Json, Router};
use pool_core::{PoolServices, PoolStats, WorkerIdentity};
use std::sync::Arc;

/// Shared API state. Holds pool services for stats and workers.
#[derive(Clone)]
pub struct ApiState {
    pub pool_services: Arc<PoolServices>,
}

/// Build the API router.
pub fn api_router(state: ApiState) -> Router {
    let state = Arc::new(state);
    Router::new()
        .route("/health", get(health))
        .route("/ready", get(ready))
        .route("/v1/pool/stats", get(pool_stats))
        .route("/v1/pool/workers", get(pool_workers))
        .route("/v1/pool/jobs/current", get(pool_jobs_current))
        .route("/v1/pool/shares/recent", get(pool_shares_recent))
        .with_state(state)
}

async fn health() -> &'static str {
    "OK"
}

async fn ready() -> &'static str {
    // Stub: always ready. Future: check daemon connectivity.
    "OK"
}

async fn pool_stats(State(state): State<Arc<ApiState>>) -> Json<PoolStats> {
    let mut stats = state.pool_services.stats.snapshot().await;
    stats.worker_count = state.pool_services.worker_registry.count().await;
    Json(stats)
}

#[derive(serde::Serialize)]
struct WorkerSummary {
    id: String,
    username: Option<String>,
}

async fn pool_workers(State(state): State<Arc<ApiState>>) -> Json<Vec<WorkerSummary>> {
    let workers = state.pool_services.worker_registry.list().await;
    let summaries: Vec<WorkerSummary> = workers
        .into_iter()
        .map(|w: WorkerIdentity| WorkerSummary {
            id: w.id,
            username: w.username,
        })
        .collect();
    Json(summaries)
}

#[derive(serde::Serialize)]
struct JobSummary {
    job_id: String,
    prev_hash: String,
    version: u32,
    nbits: u32,
    ntime: u32,
    clean_jobs: bool,
}

async fn pool_jobs_current(State(state): State<Arc<ApiState>>) -> Json<Option<JobSummary>> {
    let job = state.pool_services.job_source.current_job().await;
    Json(job.map(|j| JobSummary {
        job_id: j.job_id,
        prev_hash: hex::encode(j.prev_hash),
        version: j.version,
        nbits: j.nbits,
        ntime: j.ntime,
        clean_jobs: j.clean_jobs,
    }))
}

async fn pool_shares_recent(
    State(state): State<Arc<ApiState>>,
) -> Json<Vec<pool_core::ShareAttempt>> {
    let shares = state.pool_services.recent_shares.recent().await;
    Json(shares)
}
