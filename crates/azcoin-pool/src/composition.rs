//! Composition root: build services from config.

use coin_azcoin::{AzcoinShareValidator, NodeApiJobSource, RpcJobSource};
use common::{JobSourceMode, PoolConfig};
use pool_core::{JobSource, PoolServices, ShareValidator};
use std::sync::Arc;

/// Build job source from config. Used by main and tests.
pub fn build_job_source(config: &PoolConfig) -> Arc<dyn JobSource> {
    match config.daemon.job_source_mode {
        JobSourceMode::Rpc => Arc::new(RpcJobSource::new(&config.daemon)),
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
    Arc::new(PoolServices::new_with_validator(
        &config.pool.name,
        job_source,
        share_validator,
        config.pool.initial_difficulty,
    ))
}
