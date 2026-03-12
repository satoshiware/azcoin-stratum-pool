//! Block submission trait and adapter.

use async_trait::async_trait;
use pool_core::{BlockCandidate, BlockSubmitter};

/// AZCOIN block submitter. Wraps daemon client.
pub struct AzcoinBlockSubmitter {
    daemon: super::DaemonClient,
}

impl AzcoinBlockSubmitter {
    pub fn new(
        daemon_url: impl Into<String>,
        rpc_user: impl Into<String>,
        rpc_password: impl Into<String>,
    ) -> Self {
        Self {
            daemon: super::DaemonClient::new(daemon_url, rpc_user, rpc_password),
        }
    }
}

#[async_trait]
impl BlockSubmitter for AzcoinBlockSubmitter {
    async fn submit_block(&self, block: BlockCandidate) -> Result<bool, String> {
        let hex_block = hex::encode(&block.raw_block);
        self.daemon
            .submit_block(&hex_block)
            .await
            .map_err(|e| e.to_string())
    }
}
