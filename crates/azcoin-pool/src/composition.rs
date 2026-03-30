//! Composition root: build services from config.

use coin_azcoin::{node_api::NodeApiClient, AzcoinShareValidator, NodeApiJobSource, RpcJobSource};
use common::{JobSourceMode, PoolConfig};
use pool_core::{JobSource, PoolServices, ShareSink, ShareValidator};
use std::sync::Arc;

/// Build job source from config. Used by main and tests.
pub fn build_job_source(config: &PoolConfig) -> Arc<dyn JobSource> {
    let payout_script_pubkey = config.pool.payout_script_pubkey_bytes().ok().flatten();
    match config.daemon.job_source_mode {
        JobSourceMode::Rpc => Arc::new(RpcJobSource::new(&config.daemon, payout_script_pubkey)),
        JobSourceMode::Api => Arc::new(NodeApiJobSource::new(
            &config.daemon.url,
            if config.daemon.node_api_token.is_empty() {
                None
            } else {
                Some(config.daemon.node_api_token.clone())
            },
        )),
    }
}

/// Build pool services from config. Uses AZCOIN share validator for crypto validation.
pub fn build_pool_services(config: &PoolConfig) -> Arc<PoolServices> {
    let job_source = build_job_source(config);
    let share_validator: Arc<dyn ShareValidator> = Arc::new(AzcoinShareValidator::new());
    let share_sink: Option<Arc<dyn ShareSink>> = if config.daemon.share_api_url.trim().is_empty() {
        None
    } else {
        Some(Arc::new(NodeApiClient::new(
            &config.daemon.share_api_url,
            if config.daemon.node_api_token.is_empty() {
                None
            } else {
                Some(config.daemon.node_api_token.clone())
            },
        )))
    };
    Arc::new(PoolServices::new_with_validator_and_share_sink(
        &config.pool.name,
        job_source,
        share_validator,
        config.pool.initial_difficulty,
        share_sink,
    ))
}
