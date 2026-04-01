//! Stratum V1 response builders for subscribe/authorize.

use crate::messages::{Sv1Response, Sv1VersionRollingConfig};

const SV1_VERSION_ROLLING_MASK: u32 = 0x1fffe000;

pub fn negotiate_version_rolling(
    requested: &Sv1VersionRollingConfig,
) -> Option<Sv1VersionRollingConfig> {
    let mask = SV1_VERSION_ROLLING_MASK & requested.mask;
    if mask == 0 {
        return None;
    }

    let available_bits = mask.count_ones();
    Some(Sv1VersionRollingConfig {
        mask,
        min_bit_count: requested.min_bit_count.min(available_bits),
    })
}

/// Configure response for requested extensions, including negotiated version-rolling when enabled.
pub fn build_configure_response(
    id: Option<serde_json::Value>,
    extensions: &[String],
    version_rolling: Option<&Sv1VersionRollingConfig>,
) -> Sv1Response {
    let mut result = serde_json::Map::new();
    for extension in extensions {
        if extension == "version-rolling" {
            match version_rolling {
                Some(config) => {
                    result.insert(extension.clone(), serde_json::json!(true));
                    result.insert(
                        "version-rolling.mask".to_string(),
                        serde_json::json!(format!("{:08x}", config.mask)),
                    );
                    result.insert(
                        "version-rolling.min-bit-count".to_string(),
                        serde_json::json!(config.min_bit_count),
                    );
                }
                None => {
                    result.insert(extension.clone(), serde_json::json!(false));
                    result.insert(
                        "version-rolling.mask".to_string(),
                        serde_json::json!("00000000"),
                    );
                    result.insert(
                        "version-rolling.min-bit-count".to_string(),
                        serde_json::json!(0),
                    );
                }
            }
        } else {
            result.insert(extension.clone(), serde_json::json!(false));
        }
    }

    Sv1Response {
        id,
        result: Some(serde_json::Value::Object(result)),
        error: None,
    }
}

/// Subscribe response: [subscription_details, extranonce1, extranonce2_size]
pub fn build_subscribe_response(
    id: Option<serde_json::Value>,
    extranonce1: &str,
) -> Sv1Response {
    Sv1Response {
        id,
        result: Some(serde_json::json!([
            [["mining.set_difficulty", "mining.notify"], extranonce1, 4],
            extranonce1,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_negotiate_version_rolling_intersects_requested_mask() {
        let negotiated = negotiate_version_rolling(&Sv1VersionRollingConfig {
            mask: 0xffffffff,
            min_bit_count: 2,
        })
        .unwrap();

        assert_eq!(
            negotiated,
            Sv1VersionRollingConfig {
                mask: 0x1fffe000,
                min_bit_count: 2,
            }
        );
    }

    #[test]
    fn test_build_configure_response_enables_negotiated_version_rolling() {
        let response = build_configure_response(
            Some(serde_json::json!(7)),
            &[
                "version-rolling".to_string(),
                "minimum-difficulty".to_string(),
                "subscribe-extranonce".to_string(),
            ],
            Some(&Sv1VersionRollingConfig {
                mask: 0x1fffe000,
                min_bit_count: 2,
            }),
        );

        assert_eq!(response.id, Some(serde_json::json!(7)));
        assert!(response.error.is_none());
        assert_eq!(
            response.result,
            Some(serde_json::json!({
                "version-rolling": true,
                "version-rolling.mask": "1fffe000",
                "version-rolling.min-bit-count": 2,
                "minimum-difficulty": false,
                "subscribe-extranonce": false
            }))
        );
    }

    #[test]
    fn test_build_configure_response_version_rolling_only_when_negotiated() {
        let response = build_configure_response(
            Some(serde_json::json!(1)),
            &["version-rolling".to_string()],
            Some(&Sv1VersionRollingConfig {
                mask: 0x1fffe000,
                min_bit_count: 2,
            }),
        );

        assert_eq!(
            response.result,
            Some(serde_json::json!({
                "version-rolling": true,
                "version-rolling.mask": "1fffe000",
                "version-rolling.min-bit-count": 2
            }))
        );
    }

    #[test]
    fn test_build_configure_response_version_rolling_serializes_to_expected_json() {
        let response = build_configure_response(
            Some(serde_json::json!(1)),
            &["version-rolling".to_string()],
            Some(&Sv1VersionRollingConfig {
                mask: 0x1fffe000,
                min_bit_count: 2,
            }),
        );
        let json = serde_json::to_string(&response).unwrap();

        assert_eq!(
            json,
            r#"{"id":1,"result":{"version-rolling":true,"version-rolling.mask":"1fffe000","version-rolling.min-bit-count":2},"error":null}"#
        );
    }
}
