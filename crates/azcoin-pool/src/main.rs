//! AZCOIN mining pool main binary.
//! Bootstrap: load config, init logging, construct services, start API + SV1 listener.

use anyhow::Result;
use api_server::{api_router, ApiState};
use common::{init_tracing, load_config, JobSourceMode};
use pool_core::{JobSource, ShareProcessor, WorkerIdentity};
use protocol_sv1::{run_stratum_listener, SessionEventHandler};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpListener;
use tracing::info;

use azcoin_pool::composition::build_pool_services;

/// Session handler: stats, workers, job source, share processor.
struct Sv1SessionHandler {
    stats: Arc<pool_core::InMemoryStatsSnapshot>,
    worker_registry: Arc<pool_core::InMemoryWorkerRegistry>,
    job_source: Arc<dyn JobSource>,
    job_registry: Arc<pool_core::ActiveJobRegistry>,
    share_processor: Arc<dyn ShareProcessor>,
}

#[async_trait::async_trait]
impl SessionEventHandler for Sv1SessionHandler {
    fn on_connect(&self, _peer: SocketAddr) {
        self.stats.record_connection();
    }

    fn on_disconnect(&self, _peer: SocketAddr) {
        self.stats.record_disconnection();
    }

    async fn on_authorize(&self, username: &str) -> Result<Option<pool_core::Job>, String> {
        let worker = WorkerIdentity::new(username);
        self.worker_registry.register(worker).await;
        Ok(self.job_source.current_job().await)
    }

    async fn on_notify_sent(&self, job: pool_core::Job) {
        self.job_registry.register(job).await;
    }

    async fn on_submit(&self, share: pool_core::ShareSubmission) -> pool_core::ShareResult {
        self.share_processor.process_share(share).await
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    init_tracing();
    let config = load_config(None).map_err(|e| anyhow::anyhow!("{}", e))?;

    let job_source_mode = config.daemon.job_source_mode;
    info!(
        pool = %config.pool.name,
        api = %format!("{}:{}", config.api.bind, config.api.port),
        stratum = %format!("{}:{}", config.stratum.bind, config.stratum.port),
        daemon = %config.daemon.url,
        job_source = %job_source_mode,
        "azcoin-pool starting"
    );

    match job_source_mode {
        JobSourceMode::Rpc => info!("job source: RPC (getblocktemplate)"),
        JobSourceMode::Api => info!("job source: Node API (GET /v1/az/mining/template/current)"),
    }

    let pool_services = build_pool_services(&config);

    // SV1 session handler: stats + workers + job source + share processor
    let sv1_handler: Arc<dyn SessionEventHandler> = Arc::new(Sv1SessionHandler {
        stats: Arc::clone(&pool_services.stats),
        worker_registry: Arc::clone(&pool_services.worker_registry),
        job_source: Arc::clone(&pool_services.job_source),
        job_registry: Arc::clone(&pool_services.job_registry),
        share_processor: Arc::clone(&pool_services.share_processor),
    });

    // Start SV1 listener in background
    let stratum_bind = config.stratum.bind.clone();
    let stratum_port = config.stratum.port;
    tokio::spawn(async move {
        if let Err(e) = run_stratum_listener(&stratum_bind, stratum_port, sv1_handler).await {
            tracing::error!(error = %e, "Stratum listener failed");
        }
    });

    // API server
    let api_state = ApiState {
        pool_services: Arc::clone(&pool_services),
    };
    let app = api_router(api_state);
    let addr = format!("{}:{}", config.api.bind, config.api.port);
    let listener = TcpListener::bind(&addr).await?;
    info!(addr = %addr, "API server listening");

    axum::serve(listener, app).await?;
    Ok(())
}
