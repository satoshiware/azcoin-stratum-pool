//! Block template provider trait and adapter.

use async_trait::async_trait;
use pool_core::Job;

/// Provides block templates as Jobs. Implemented by coin_azcoin via daemon.
#[async_trait]
pub trait BlockTemplateProvider: Send + Sync {
    async fn get_template(&self) -> Option<Job>;
}

/// Stub implementation. Returns None until daemon integration is complete.
pub struct AzcoinBlockTemplateProvider {
    _daemon_url: String,
}

impl AzcoinBlockTemplateProvider {
    pub fn new(daemon_url: impl Into<String>) -> Self {
        Self {
            _daemon_url: daemon_url.into(),
        }
    }
}

#[async_trait]
impl BlockTemplateProvider for AzcoinBlockTemplateProvider {
    async fn get_template(&self) -> Option<Job> {
        // TODO: Call daemon getblocktemplate, convert to Job
        None
    }
}
