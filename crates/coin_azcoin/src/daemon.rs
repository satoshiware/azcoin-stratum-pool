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
    pub coinbaseaux: Option<CoinbaseAux>,
}

#[derive(Debug, Deserialize)]
pub struct TransactionEntry {
    pub data: String,
    pub txid: Option<String>,
    pub hash: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CoinbaseAux {
    pub flags: Option<String>,
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
            .rpc_call(
                "getblocktemplate",
                serde_json::json!([{ "rules": ["segwit"] }]),
            )
            .await
            .map_err(|e| {
                warn!(error = %e, "daemon getblocktemplate failed");
                e
            })?;

        let template: BlockTemplate = serde_json::from_value(result)
            .map_err(|e| PoolError::Daemon(format!("daemon template parse failed: {}", e)))?;

        Ok(Some(template))
    }

    /// Submit a fully assembled raw block to the daemon via submitblock.
    pub async fn submit_block(&self, raw_block: &[u8]) -> Result<bool, PoolError> {
        let result = self
            .rpc_call("submitblock", serde_json::json!([hex::encode(raw_block)]))
            .await?;

        match result {
            serde_json::Value::Null => Ok(true),
            serde_json::Value::String(reason) if reason.trim().is_empty() => Ok(true),
            serde_json::Value::String(reason) => {
                Err(PoolError::Daemon(format!("submitblock rejected: {}", reason)))
            }
            other => Err(PoolError::Daemon(format!(
                "submitblock returned unexpected result: {}",
                other
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mockito::Matcher;

    fn build_runtime() -> tokio::runtime::Runtime {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
    }

    #[test]
    fn test_submit_block_success_when_rpc_returns_null() {
        let mut server = mockito::Server::new();
        let raw_block = vec![0x01, 0x02, 0x03, 0x04];
        let mock = server
            .mock("POST", "/")
            .match_body(Matcher::Regex(r#""method":"submitblock""#.to_string()))
            .match_body(Matcher::Regex(r#""01020304""#.to_string()))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"result":null,"error":null,"id":"azcoin-pool"}"#)
            .create();

        let client = DaemonClient::new(server.url(), "", "");
        let result = build_runtime().block_on(client.submit_block(&raw_block));
        mock.assert();

        assert_eq!(result.unwrap(), true);
    }

    #[test]
    fn test_submit_block_reject_when_rpc_returns_reason_string() {
        let mut server = mockito::Server::new();
        server
            .mock("POST", "/")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"result":"high-hash","error":null,"id":"azcoin-pool"}"#)
            .create();

        let client = DaemonClient::new(server.url(), "", "");
        let err = build_runtime()
            .block_on(client.submit_block(&[0xaa, 0xbb]))
            .unwrap_err();

        assert!(matches!(err, PoolError::Daemon(_)));
        assert!(err.to_string().contains("submitblock rejected: high-hash"));
    }

    #[test]
    fn test_submit_block_unexpected_response_shape_fails_clearly() {
        let mut server = mockito::Server::new();
        server
            .mock("POST", "/")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"result":{"status":"ok"},"error":null,"id":"azcoin-pool"}"#)
            .create();

        let client = DaemonClient::new(server.url(), "", "");
        let err = build_runtime()
            .block_on(client.submit_block(&[0xaa, 0xbb]))
            .unwrap_err();

        assert!(matches!(err, PoolError::Daemon(_)));
        assert!(err
            .to_string()
            .contains("submitblock returned unexpected result"));
    }

    #[test]
    fn test_submit_block_request_payload_contains_hex_encoded_block() {
        let mut server = mockito::Server::new();
        let raw_block = vec![0xde, 0xad, 0xbe, 0xef];
        let mock = server
            .mock("POST", "/")
            .match_body(Matcher::Regex(r#""params":\["deadbeef"\]"#.to_string()))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"result":"","error":null,"id":"azcoin-pool"}"#)
            .create();

        let client = DaemonClient::new(server.url(), "", "");
        let result = build_runtime().block_on(client.submit_block(&raw_block));
        mock.assert();

        assert_eq!(result.unwrap(), true);
    }
}
