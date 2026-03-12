//! Daemon/RPC client. Config-driven, getblocktemplate for block template fetching.

use common::PoolError;
use serde::Deserialize;
use tracing::{debug, warn};

/// Daemon RPC client. Config-driven via URL and optional credentials.
pub struct DaemonClient {
    url: String,
    rpc_user: String,
    rpc_password: String,
    client: reqwest::Client,
}

/// Minimal getblocktemplate response. Bitcoin-style fields.
/// TODO: AZCOIN-specific template details may need refinement.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BlockTemplate {
    pub version: u32,
    pub previousblockhash: String,
    /// Compressed difficulty.
    pub bits: String,
    pub curtime: u64,
    pub height: u64,
    pub transactions: Vec<TransactionEntry>,
    pub coinbasevalue: u64,
}

#[derive(Debug, Deserialize)]
pub struct TransactionEntry {
    pub data: String,
    pub txid: Option<String>,
    pub hash: Option<String>,
}

impl DaemonClient {
    pub fn new(
        url: impl Into<String>,
        rpc_user: impl Into<String>,
        rpc_password: impl Into<String>,
    ) -> Self {
        Self {
            url: url.into().trim_end_matches('/').to_string(),
            rpc_user: rpc_user.into(),
            rpc_password: rpc_password.into(),
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(10))
                .build()
                .unwrap_or_default(),
        }
    }

    /// JSON-RPC call.
    async fn rpc_call(
        &self,
        method: &str,
        params: serde_json::Value,
    ) -> Result<serde_json::Value, PoolError> {
        let url = format!("{}/", self.url);
        let body = serde_json::json!({
            "jsonrpc": "1.0",
            "id": "azcoin-pool",
            "method": method,
            "params": params
        });

        let mut req = self.client.post(&url).json(&body);
        if !self.rpc_user.is_empty() {
            req = req.basic_auth(&self.rpc_user, Some(&self.rpc_password));
        }

        let resp = req
            .send()
            .await
            .map_err(|e| PoolError::Daemon(format!("daemon request failed: {}", e)))?;

        let status = resp.status();
        let json: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| PoolError::Daemon(format!("daemon response parse failed: {}", e)))?;

        if let Some(err) = json.get("error") {
            if !err.is_null() {
                let msg = err
                    .get("message")
                    .and_then(|m| m.as_str())
                    .unwrap_or("unknown");
                return Err(PoolError::Daemon(format!("daemon RPC error: {}", msg)));
            }
        }

        if !status.is_success() {
            return Err(PoolError::Daemon(format!(
                "daemon returned HTTP {}",
                status
            )));
        }

        json.get("result")
            .cloned()
            .ok_or_else(|| PoolError::Daemon("daemon response missing result".into()))
    }

    /// Fetch block template from daemon. Returns Ok(Some(template)) if successful.
    pub async fn get_block_template(&self) -> Result<Option<BlockTemplate>, PoolError> {
        debug!(url = %self.url, "daemon getblocktemplate");
        // TODO: AZCOIN may use different params. Bitcoin uses {} or {"rules": ["segwit"]}.
        let result = self
            .rpc_call("getblocktemplate", serde_json::json!([{}]))
            .await
            .map_err(|e| {
                warn!(error = %e, "daemon getblocktemplate failed");
                e
            })?;

        let template: BlockTemplate = serde_json::from_value(result)
            .map_err(|e| PoolError::Daemon(format!("daemon template parse failed: {}", e)))?;

        Ok(Some(template))
    }

    /// Placeholder: submit block to daemon.
    pub async fn submit_block(&self, _hex_block: &str) -> Result<bool, PoolError> {
        Err(PoolError::Daemon("submit_block not implemented".into()))
    }
}
