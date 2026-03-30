//! AZCOIN node REST API client. GET /v1/az/mining/template/current for block template.

use async_trait::async_trait;
use chrono::Utc;
use common::PoolError;
use pool_core::{ShareResult, ShareSink, ShareSubmission};
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

/// Node API client. Fetches template via REST GET.
pub struct NodeApiClient {
    base_url: String,
    bearer_token: Option<String>,
    client: reqwest::Client,
}

/// Response from GET /v1/az/mining/template/current.
/// Matches the AZCOIN node API contract exactly.
#[derive(Debug, Deserialize)]
pub struct NodeApiTemplate {
    pub job_id: String,
    pub prev_hash: String,
    pub version: u32,
    pub nbits: String,
    /// Block time as hex string (e.g. "69b33a70"). Parsed to u32 for pool_core::Job.
    pub ntime: String,
    pub clean_jobs: bool,
    pub height: u64,
}

#[derive(Debug, Serialize)]
struct NodeApiShareRequest {
    ts: i64,
    worker: String,
    job_id: String,
    extranonce2: String,
    ntime: String,
    nonce: String,
    accepted: bool,
    duplicate: bool,
    share_diff: u32,
    reason: String,
}

impl NodeApiClient {
    /// Create client. Pass non-empty token for Bearer auth.
    pub fn new(base_url: impl Into<String>, bearer_token: Option<String>) -> Self {
        let base_url = base_url.into().trim_end_matches('/').to_string();
        let bearer_token = bearer_token.and_then(|t| {
            let t = t.trim();
            if t.is_empty() {
                None
            } else {
                Some(t.to_string())
            }
        });
        Self {
            base_url,
            bearer_token,
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(10))
                .build()
                .unwrap_or_default(),
        }
    }

    /// Whether Bearer auth is configured.
    pub fn auth_configured(&self) -> bool {
        self.bearer_token.is_some()
    }

    /// Fetch current block template from node API.
    pub async fn get_template_current(&self) -> Result<Option<NodeApiTemplate>, PoolError> {
        let url = format!("{}/v1/az/mining/template/current", self.base_url);
        let auth_configured = self.auth_configured();
        info!(
            url = %url,
            auth_configured = auth_configured,
            "node API get template"
        );

        let mut req = self.client.get(&url);
        if let Some(ref token) = self.bearer_token {
            req = req.bearer_auth(token);
        }

        let resp = req
            .send()
            .await
            .map_err(|e| PoolError::Daemon(format!("node API request failed: {}", e)))?;

        let status = resp.status();
        if !status.is_success() {
            warn!(
                url = %url,
                auth_configured = auth_configured,
                status = %status,
                "node API returned non-2xx"
            );
            return Err(PoolError::Daemon(format!(
                "node API returned HTTP {}",
                status
            )));
        }

        let template: NodeApiTemplate = resp
            .json()
            .await
            .map_err(|e| PoolError::Daemon(format!("node API response parse failed: {}", e)))?;

        info!(
            job_id = %template.job_id,
            height = template.height,
            "node API template success"
        );
        Ok(Some(template))
    }

    async fn post_share(&self, payload: &NodeApiShareRequest) -> Result<(), PoolError> {
        let url = format!("{}/v1/mining/share", self.base_url);
        let mut req = self.client.post(&url).json(payload);
        if let Some(ref token) = self.bearer_token {
            req = req.bearer_auth(token);
        }

        let resp = req
            .send()
            .await
            .map_err(|e| PoolError::Daemon(format!("node API share POST failed: {}", e)))?;

        let status = resp.status();
        if !status.is_success() {
            warn!(url = %url, status = %status, "node API share POST returned non-2xx");
            return Err(PoolError::Daemon(format!(
                "node API share POST returned HTTP {}",
                status
            )));
        }

        Ok(())
    }
}

fn build_share_request(
    share: &ShareSubmission,
    result: &ShareResult,
    pool_difficulty: u32,
) -> NodeApiShareRequest {
    NodeApiShareRequest {
        ts: Utc::now().timestamp(),
        worker: share.worker.id.clone(),
        job_id: share.job_id.clone(),
        extranonce2: hex::encode(&share.extra_nonce2),
        ntime: format!("{:08x}", share.ntime),
        nonce: format!("{:08x}", share.nonce),
        accepted: result.is_accepted(),
        duplicate: false,
        share_diff: pool_difficulty,
        reason: result.reject_reason().unwrap_or_default(),
    }
}

#[async_trait]
impl ShareSink for NodeApiClient {
    async fn submit_share(
        &self,
        share: &ShareSubmission,
        result: &ShareResult,
        pool_difficulty: u32,
    ) -> Result<(), String> {
        let payload = build_share_request(share, result, pool_difficulty);
        self.post_share(&payload).await.map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mockito::Matcher;

    fn example_share() -> ShareSubmission {
        ShareSubmission {
            job_id: "job-1".to_string(),
            worker: pool_core::WorkerIdentity::new("miner.worker"),
            extra_nonce2: vec![0xaa, 0xbb, 0xcc, 0xdd],
            ntime: 0x69b33a70,
            nonce: 0x01020304,
            validation_context: None,
        }
    }

    /// Real API payload shape from AZCOIN node.
    const REAL_API_PAYLOAD: &str = r#"{
        "job_id":"c68fdce62b92e2d8",
        "prev_hash":"0000000000000000c589462bc769be8b4a12fddd736d5bd5e47966e10421222b",
        "version":536870912,
        "nbits":"1a020e7c",
        "ntime":"69b33a70",
        "clean_jobs":true,
        "height":808523
    }"#;

    #[test]
    fn test_deserialize_real_api_payload() {
        let template: NodeApiTemplate = serde_json::from_str(REAL_API_PAYLOAD).unwrap();
        assert_eq!(template.job_id, "c68fdce62b92e2d8");
        assert_eq!(
            template.prev_hash,
            "0000000000000000c589462bc769be8b4a12fddd736d5bd5e47966e10421222b"
        );
        assert_eq!(template.version, 536870912);
        assert_eq!(template.nbits, "1a020e7c");
        assert_eq!(template.ntime, "69b33a70");
        assert!(template.clean_jobs);
        assert_eq!(template.height, 808523);
    }

    #[test]
    fn test_bearer_auth_header_present_and_formatted() {
        let mut server = mockito::Server::new();
        let mock = server
            .mock("GET", "/v1/az/mining/template/current")
            .match_header("authorization", "Bearer testtoken-123")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(REAL_API_PAYLOAD)
            .create();

        let client = NodeApiClient::new(server.url(), Some("testtoken-123".to_string()));
        assert!(client.auth_configured());

        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let result = rt.block_on(client.get_template_current());
        mock.assert();

        let template = result.unwrap().unwrap();
        assert_eq!(template.height, 808523);
        assert_eq!(template.job_id, "c68fdce62b92e2d8");
    }

    #[test]
    fn test_without_token_succeeds_when_server_does_not_require_auth() {
        let mut server = mockito::Server::new();
        server
            .mock("GET", "/v1/az/mining/template/current")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(REAL_API_PAYLOAD)
            .create();

        let client = NodeApiClient::new(server.url(), None);
        assert!(!client.auth_configured());

        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let result = rt.block_on(client.get_template_current());
        assert!(result.is_ok());
    }

    #[test]
    fn test_build_share_request_matches_live_contract() {
        let share = example_share();

        let accepted = build_share_request(&share, &ShareResult::Accepted, 32);
        assert!(accepted.ts > 0);
        assert_eq!(accepted.worker, "miner.worker");
        assert_eq!(accepted.job_id, "job-1");
        assert_eq!(accepted.extranonce2, "aabbccdd");
        assert_eq!(accepted.ntime, "69b33a70");
        assert_eq!(accepted.nonce, "01020304");
        assert!(accepted.accepted);
        assert!(!accepted.duplicate);
        assert_eq!(accepted.share_diff, 32);
        assert_eq!(accepted.reason, "");

        let rejected = build_share_request(
            &share,
            &ShareResult::Rejected {
                reason: "low diff".to_string(),
            },
            32,
        );
        assert!(!rejected.accepted);
        assert_eq!(rejected.reason, "low diff");
    }

    #[test]
    fn test_post_share_uses_expected_path_and_auth() {
        let mut server = mockito::Server::new();
        let payload = NodeApiShareRequest {
            ts: 1_743_339_296,
            worker: "miner.worker".to_string(),
            job_id: "job-1".to_string(),
            extranonce2: "aabbccdd".to_string(),
            ntime: "69b33a70".to_string(),
            nonce: "01020304".to_string(),
            accepted: true,
            duplicate: false,
            share_diff: 32,
            reason: String::new(),
        };
        let mock = server
            .mock("POST", "/v1/mining/share")
            .match_header("authorization", "Bearer testtoken-123")
            .match_header("content-type", Matcher::Regex("application/json".to_string()))
            .match_body(Matcher::PartialJson(serde_json::json!({
                "ts": 1_743_339_296,
                "worker": "miner.worker",
                "job_id": "job-1",
                "extranonce2": "aabbccdd",
                "ntime": "69b33a70",
                "nonce": "01020304",
                "accepted": true,
                "duplicate": false,
                "share_diff": 32,
                "reason": ""
            })))
            .with_status(200)
            .create();

        let client = NodeApiClient::new(server.url(), Some("testtoken-123".to_string()));
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let result = rt.block_on(client.post_share(&payload));
        mock.assert();
        assert!(result.is_ok());
    }
}
