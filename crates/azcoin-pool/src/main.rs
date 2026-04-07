//! AZCOIN mining pool main binary.
//! Bootstrap: load config, init logging, construct services, start API + SV1 listener.

use anyhow::Result;
use api_server::{api_router, ApiState};
use azcoin_pool::sv1_handler::Sv1SessionHandler;
use coin_azcoin::AzcoinBlockSubmitter;
use common::{init_tracing, load_config, JobSourceMode};
use pool_core::BlockSubmitter;
use protocol_sv1::{run_stratum_listener, SessionEventHandler};
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::broadcast;
use tracing::{info, warn};

use azcoin_pool::composition::build_pool_services;

#[tokio::main]
async fn main() -> Result<()> {
    init_tracing();
    let config = load_config(None).map_err(|e| anyhow::anyhow!("{}", e))?;

    let job_source_mode = config.daemon.job_source_mode;
    info!(
        pool = %config.pool.name,
        initial_difficulty = config.pool.initial_difficulty,
        api = %format!("{}:{}", config.api.bind, config.api.port),
        stratum = %format!("{}:{}", config.stratum.bind, config.stratum.port),
        daemon = %config.daemon.url,
        job_source = %job_source_mode,
        "azcoin-pool starting"
    );

    match job_source_mode {
        JobSourceMode::Rpc => info!("job source: RPC (getblocktemplate)"),
        JobSourceMode::Api => {
            info!("job source: Node API (GET /v1/az/mining/template/current)");
            warn!("API mode sources jobs from the node API, but live submitblock still targets daemon.url as JSON-RPC; use RPC mode for end-to-end block submission validation unless the same base URL serves both");
        }
    }

    let pool_services = build_pool_services(&config);
    let payout_script_pubkey = config
        .pool
        .payout_script_pubkey_bytes()
        .map_err(|e| anyhow::anyhow!("{}", e))?;
    if payout_script_pubkey.is_some() {
        info!("block-found submission is armed: pool payout scriptPubKey is configured");
    } else {
        warn!(
            "block-found submission is disabled: pool.payout_script_pubkey_hex is not configured"
        );
    }
    let block_submitter: Arc<dyn BlockSubmitter> = Arc::new(AzcoinBlockSubmitter::new(
        &config.daemon.url,
        &config.daemon.rpc_user,
        &config.daemon.rpc_password,
    ));

    // SV1 session handler: stats + workers + job source + share processor
    let sv1_handler: Arc<dyn SessionEventHandler> = Arc::new(Sv1SessionHandler {
        stats: Arc::clone(&pool_services.stats),
        worker_registry: Arc::clone(&pool_services.worker_registry),
        job_source: Arc::clone(&pool_services.job_source),
        job_registry: Arc::clone(&pool_services.job_registry),
        share_processor: Arc::clone(&pool_services.share_processor),
        block_submitter,
        payout_script_pubkey,
    });

    // Broadcast channel for pushing new jobs to all connected SV1 sessions
    let (job_tx, _) = broadcast::channel::<pool_core::Job>(16);

    // Job poller: detect template changes and broadcast to sessions
    {
        let poller_source = Arc::clone(&pool_services.job_source);
        let poller_tx = job_tx.clone();
        tokio::spawn(async move {
            let mut last_job_id = String::new();
            let mut last_height: u64 = 0;
            let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(5));
            loop {
                interval.tick().await;
                let job = match poller_source.current_job().await {
                    Some(j) => j,
                    None => continue,
                };
                let height = job
                    .block_assembly
                    .as_ref()
                    .map(|b| b.height)
                    .unwrap_or(0);
                if job.job_id != last_job_id || height != last_height {
                    info!(
                        job_id = %job.job_id,
                        height,
                        prev_job_id = %last_job_id,
                        prev_height = last_height,
                        "new job detected, broadcasting to sessions"
                    );
                    last_job_id = job.job_id.clone();
                    last_height = height;
                    let _ = poller_tx.send(job);
                }
            }
        });
    }

    // Start SV1 listener in background
    let stratum_bind = config.stratum.bind.clone();
    let stratum_port = config.stratum.port;
    let initial_difficulty = config.pool.initial_difficulty;
    tokio::spawn(async move {
        if let Err(e) = run_stratum_listener(
            &stratum_bind,
            stratum_port,
            sv1_handler,
            initial_difficulty,
            job_tx,
        )
        .await
        {
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
