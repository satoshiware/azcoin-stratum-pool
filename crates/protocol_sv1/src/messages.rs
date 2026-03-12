//! Stratum V1 request/response message types.

use serde::{Deserialize, Serialize};

/// Stratum V1 JSON-RPC request (miner → pool).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Sv1Request {
    pub id: Option<serde_json::Value>,
    pub method: String,
    pub params: Option<serde_json::Value>,
}

/// Stratum V1 JSON-RPC response (pool → miner).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Sv1Response {
    pub id: Option<serde_json::Value>,
    pub result: Option<serde_json::Value>,
    pub error: Option<Sv1Error>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Sv1Error {
    pub code: i32,
    pub message: String,
}

/// Internal domain command produced by parsing SV1 requests.
#[derive(Debug, Clone)]
pub enum Sv1DomainCommand {
    Subscribe,
    Authorize {
        username: String,
        password: String,
    },
    SubmitShare {
        username: String,
        job_id: String,
        extra_nonce2: Vec<u8>,
        ntime: u32,
        nonce: u32,
    },
}
