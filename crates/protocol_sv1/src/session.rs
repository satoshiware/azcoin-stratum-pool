//! Stratum V1 response builders for subscribe/authorize.

use crate::messages::Sv1Response;

/// Subscribe response: [subscription_details, extranonce1, extranonce2_size]
pub fn build_subscribe_response(id: Option<serde_json::Value>) -> Sv1Response {
    Sv1Response {
        id,
        result: Some(serde_json::json!([
            [["mining.set_difficulty", "mining.notify"], "00000000", 4],
            "00000000",
            4
        ])),
        error: None,
    }
}

/// Authorize success response.
pub fn build_authorize_success(id: Option<serde_json::Value>) -> Sv1Response {
    Sv1Response {
        id,
        result: Some(serde_json::json!(true)),
        error: None,
    }
}

/// Build mining.set_difficulty notification (stub difficulty).
pub fn build_set_difficulty_notification(difficulty: u32) -> serde_json::Value {
    serde_json::json!({
        "method": "mining.set_difficulty",
        "params": [difficulty]
    })
}

/// Submit success response.
pub fn build_submit_success(id: Option<serde_json::Value>) -> Sv1Response {
    Sv1Response {
        id,
        result: Some(serde_json::json!(true)),
        error: None,
    }
}

/// Submit reject response.
pub fn build_submit_reject(id: Option<serde_json::Value>, reason: &str) -> Sv1Response {
    build_error_response(id, -1, reason)
}

/// Generic JSON-RPC error response.
pub fn build_error_response(
    id: Option<serde_json::Value>,
    code: i32,
    message: &str,
) -> Sv1Response {
    Sv1Response {
        id,
        result: None,
        error: Some(crate::messages::Sv1Error {
            code,
            message: message.to_string(),
        }),
    }
}
