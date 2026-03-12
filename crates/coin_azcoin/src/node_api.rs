//! AZCOIN node REST API client. GET /v1/az/mining/template/current for block template.

use common::PoolError;
use serde::Deserialize;
use tracing::debug;

/// Node API client. Fetches template via REST GET.
pub struct NodeApiClient {
    base_url: String,
    client: reqwest::Client,
}

/// Response from GET /v1/az/mining/template/current.
/// TODO: AZCOIN-specific fields may need refinement.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeApiTemplate {
    pub version: u32,
    #[serde(alias = "previousblockhash")]
    pub previous_block_hash: String,
    pub bits: String,
    /// Unix timestamp. Normalized to u32 for pool_core::Job.
    pub curtime: u64,
    pub height: u64,
    #[serde(default)]
    pub transactions: Vec<NodeApiTxEntry>,
    #[serde(default)]
    pub coinbase_value: u64,
}

#[derive(Debug, Deserialize)]
pub struct NodeApiTxEntry {
    pub data: Option<String>,
    pub txid: Option<String>,
    pub hash: Option<String>,
}

impl NodeApiClient {
    pub fn new(base_url: impl Into<String>) -> Self {
        let base_url = base_url.into().trim_end_matches('/').to_string();
        Self {
            base_url,
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(10))
                .build()
                .unwrap_or_default(),
        }
    }

    /// Fetch current block template from node API.
    pub async fn get_template_current(&self) -> Result<Option<NodeApiTemplate>, PoolError> {
        let url = format!("{}/v1/az/mining/template/current", self.base_url);
        debug!(url = %url, "node API get template");

        let resp = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| PoolError::Daemon(format!("node API request failed: {}", e)))?;

        if !resp.status().is_success() {
            return Err(PoolError::Daemon(format!(
                "node API returned HTTP {}",
                resp.status()
            )));
        }

        let template: NodeApiTemplate = resp
            .json()
            .await
            .map_err(|e| PoolError::Daemon(format!("node API response parse failed: {}", e)))?;

        Ok(Some(template))
    }
}
