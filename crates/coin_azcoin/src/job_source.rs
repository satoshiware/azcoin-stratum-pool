//! JobSource implementations: RPC (getblocktemplate) and Node API (REST).

use async_trait::async_trait;
use common::DaemonSection;
use pool_core::{Job, JobSource};
use std::sync::Arc;
use tracing::{error, info};

use crate::api_template_mapper::api_template_to_job;
use crate::daemon::DaemonClient;
use crate::node_api::NodeApiClient;
use crate::template_mapper::template_to_job;

/// JobSource backed by AZCOIN daemon JSON-RPC getblocktemplate.
pub struct RpcJobSource {
    daemon: Arc<DaemonClient>,
}

impl RpcJobSource {
    pub fn new(daemon_config: &DaemonSection) -> Self {
        let daemon = Arc::new(DaemonClient::new(
            daemon_config.url.clone(),
            daemon_config.rpc_user.clone(),
            daemon_config.rpc_password.clone(),
        ));
        Self { daemon }
    }
}

#[async_trait]
impl JobSource for RpcJobSource {
    async fn current_job(&self) -> Option<Job> {
        match self.daemon.get_block_template().await {
            Ok(Some(template)) => match template_to_job(&template) {
                Ok(job) => {
                    info!(job_id = %job.job_id, height = template.height, "rpc job fetch success");
                    Some(job)
                }
                Err(e) => {
                    error!(error = %e, "rpc template to job mapping failed");
                    None
                }
            },
            Ok(None) => {
                error!("rpc returned no template");
                None
            }
            Err(e) => {
                error!(error = %e, "rpc getblocktemplate failed");
                None
            }
        }
    }
}

/// JobSource backed by AZCOIN node REST API GET /v1/az/mining/template/current.
pub struct NodeApiJobSource {
    client: Arc<NodeApiClient>,
}

impl NodeApiJobSource {
    /// Create with base URL and optional Bearer token for auth.
    pub fn new(base_url: impl Into<String>, bearer_token: Option<String>) -> Self {
        Self {
            client: Arc::new(NodeApiClient::new(base_url, bearer_token)),
        }
    }
}

#[async_trait]
impl JobSource for NodeApiJobSource {
    async fn current_job(&self) -> Option<Job> {
        match self.client.get_template_current().await {
            Ok(Some(template)) => match api_template_to_job(&template) {
                Ok(job) => {
                    info!(
                        job_id = %job.job_id,
                        height = template.height,
                        "node API job fetch success"
                    );
                    Some(job)
                }
                Err(e) => {
                    error!(error = %e, "node API template to job mapping failed");
                    None
                }
            },
            Ok(None) => {
                error!("node API returned no template");
                None
            }
            Err(e) => {
                error!(error = %e, "node API get template failed");
                None
            }
        }
    }
}
