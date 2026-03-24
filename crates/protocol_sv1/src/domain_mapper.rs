//! Maps Stratum V1 requests into internal domain commands.
//! Keeps SV1 wire format separate from pool_core domain types.

use crate::messages::{Sv1DomainCommand, Sv1Request, Sv1VersionRollingConfig};
use tracing::warn;

/// Parse result: Ok(Some(cmd)), Ok(None) for unknown method, Err(msg) for parse error.
pub fn map_request_to_command(req: &Sv1Request) -> Result<Option<Sv1DomainCommand>, String> {
    match req.method.as_str() {
        "mining.configure" => map_configure_params(req).map(Some),
        "mining.subscribe" => Ok(Some(Sv1DomainCommand::Subscribe)),
        "mining.authorize" => {
            let params = req
                .params
                .as_ref()
                .and_then(|p| p.as_array())
                .ok_or("mining.authorize requires params array")?;
            let username = params
                .first()
                .and_then(|v| v.as_str())
                .ok_or("mining.authorize requires username")?
                .to_string();
            let password = params
                .get(1)
                .and_then(|p| p.as_str())
                .unwrap_or("")
                .to_string();
            Ok(Some(Sv1DomainCommand::Authorize { username, password }))
        }
        "mining.submit" => map_submit_params(req).map(Some),
        _ => {
            warn!(method = %req.method, "unknown SV1 method");
            Ok(None)
        }
    }
}

fn map_configure_params(req: &Sv1Request) -> Result<Sv1DomainCommand, String> {
    let params = match req.params.as_ref() {
        None => None,
        Some(params) => Some(
            params
                .as_array()
                .ok_or("mining.configure requires params array")?,
        ),
    };
    let extensions = match params {
        None => Vec::new(),
        Some(params) => match params.first() {
            None => Vec::new(),
            Some(value) => value
                .as_array()
                .ok_or("mining.configure param 0 (extensions) must be array")?
                .iter()
                .map(|item| {
                    item.as_str()
                        .map(str::to_string)
                        .ok_or("mining.configure extensions must be strings")
                })
                .collect::<Result<Vec<_>, _>>()?,
        },
    };
    let options = params.and_then(|p| p.get(1)).and_then(|v| v.as_object());
    let version_rolling = if extensions.iter().any(|ext| ext == "version-rolling") {
        Some(parse_version_rolling_config(options)?)
    } else {
        None
    };

    Ok(Sv1DomainCommand::Configure {
        extensions,
        version_rolling,
    })
}

fn parse_version_rolling_config(
    options: Option<&serde_json::Map<String, serde_json::Value>>,
) -> Result<Sv1VersionRollingConfig, String> {
    let mask = match options.and_then(|opts| opts.get("version-rolling.mask")) {
        Some(value) => parse_hex_u32(value, "mining.configure version-rolling.mask")?,
        None => u32::MAX,
    };
    let min_bit_count = match options.and_then(|opts| opts.get("version-rolling.min-bit-count")) {
        Some(value) => {
            let count = value
                .as_u64()
                .ok_or("mining.configure version-rolling.min-bit-count must be integer")?;
            u32::try_from(count)
                .map_err(|_| "mining.configure version-rolling.min-bit-count out of range")?
        }
        None => 0,
    };

    Ok(Sv1VersionRollingConfig {
        mask,
        min_bit_count,
    })
}

fn parse_hex_u32(value: &serde_json::Value, label: &str) -> Result<u32, String> {
    let hex = value
        .as_str()
        .ok_or_else(|| format!("{} must be 8-char hex string", label))?;
    if hex.len() != 8 {
        return Err(format!("{} must be 8-char hex string", label));
    }
    u32::from_str_radix(hex, 16).map_err(|_| format!("{} invalid hex", label))
}

/// Parse mining.submit params into SubmitShare. Returns Err with explicit reason on failure.
fn map_submit_params(req: &Sv1Request) -> Result<Sv1DomainCommand, String> {
    let params = req
        .params
        .as_ref()
        .and_then(|p| p.as_array())
        .ok_or("mining.submit requires params array")?;

    if params.len() < 5 {
        return Err(format!(
            "mining.submit requires 5 params, got {}",
            params.len()
        ));
    }

    let username = params
        .first()
        .and_then(|v| v.as_str())
        .ok_or("mining.submit param 0 (username) must be string")?
        .to_string();
    let job_id = params
        .get(1)
        .and_then(|v| v.as_str())
        .ok_or("mining.submit param 1 (job_id) must be string")?
        .to_string();
    let extra_nonce2_hex = params
        .get(2)
        .and_then(|v| v.as_str())
        .ok_or("mining.submit param 2 (extra_nonce2) must be hex string")?;
    let ntime_hex = params
        .get(3)
        .and_then(|v| v.as_str())
        .ok_or("mining.submit param 3 (ntime) must be hex string")?;
    let nonce_hex = params
        .get(4)
        .and_then(|v| v.as_str())
        .ok_or("mining.submit param 4 (nonce) must be hex string")?;

    let extra_nonce2 = hex::decode(extra_nonce2_hex)
        .map_err(|_| "mining.submit param 2 (extra_nonce2) invalid hex")?;
    if ntime_hex.len() != 8 {
        return Err("mining.submit param 3 (ntime) must be 8 hex chars".to_string());
    }
    let ntime = u32::from_str_radix(ntime_hex, 16)
        .map_err(|_| "mining.submit param 3 (ntime) invalid hex")?;
    if nonce_hex.len() != 8 {
        return Err("mining.submit param 4 (nonce) must be 8 hex chars".to_string());
    }
    let nonce = u32::from_str_radix(nonce_hex, 16)
        .map_err(|_| "mining.submit param 4 (nonce) invalid hex")?;
    let version_bits = match params.get(5) {
        Some(value) => Some(parse_hex_u32(
            value,
            "mining.submit param 5 (version_bits)",
        )?),
        None => None,
    };

    Ok(Sv1DomainCommand::SubmitShare {
        username,
        job_id,
        extra_nonce2,
        ntime,
        nonce,
        version_bits,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_configure_valid_params() {
        let req = Sv1Request {
            id: Some(serde_json::json!(1)),
            method: "mining.configure".to_string(),
            params: Some(serde_json::json!([
                ["version-rolling", "minimum-difficulty", "subscribe-extranonce"],
                {
                    "version-rolling.mask": "1fffe000",
                    "version-rolling.min-bit-count": 2
                }
            ])),
        };
        let cmd = map_request_to_command(&req).unwrap().unwrap();
        match cmd {
            Sv1DomainCommand::Configure {
                extensions,
                version_rolling,
            } => {
                assert_eq!(
                    extensions,
                    vec![
                        "version-rolling".to_string(),
                        "minimum-difficulty".to_string(),
                        "subscribe-extranonce".to_string()
                    ]
                );
                assert_eq!(
                    version_rolling,
                    Some(Sv1VersionRollingConfig {
                        mask: 0x1fffe000,
                        min_bit_count: 2,
                    })
                );
            }
            _ => panic!("expected Configure"),
        }
    }

    #[test]
    fn test_configure_missing_params_is_tolerated() {
        let req = Sv1Request {
            id: Some(serde_json::json!(1)),
            method: "mining.configure".to_string(),
            params: None,
        };
        let cmd = map_request_to_command(&req).unwrap().unwrap();
        match cmd {
            Sv1DomainCommand::Configure {
                extensions,
                version_rolling,
            } => {
                assert!(extensions.is_empty());
                assert_eq!(version_rolling, None);
            }
            _ => panic!("expected Configure"),
        }
    }

    #[test]
    fn test_configure_version_rolling_defaults_when_params_missing() {
        let req = Sv1Request {
            id: Some(serde_json::json!(1)),
            method: "mining.configure".to_string(),
            params: Some(serde_json::json!([["version-rolling"], {}])),
        };
        let cmd = map_request_to_command(&req).unwrap().unwrap();
        match cmd {
            Sv1DomainCommand::Configure {
                version_rolling, ..
            } => {
                assert_eq!(
                    version_rolling,
                    Some(Sv1VersionRollingConfig {
                        mask: u32::MAX,
                        min_bit_count: 0,
                    })
                );
            }
            _ => panic!("expected Configure"),
        }
    }

    #[test]
    fn test_submit_valid_params() {
        let req = Sv1Request {
            id: Some(serde_json::json!(1)),
            method: "mining.submit".to_string(),
            params: Some(serde_json::json!([
                "user.worker1",
                "job-123",
                "00000000",
                "69b33a70",
                "12345678"
            ])),
        };
        let cmd = map_request_to_command(&req).unwrap().unwrap();
        match cmd {
            Sv1DomainCommand::SubmitShare {
                username,
                job_id,
                extra_nonce2,
                ntime,
                nonce,
                version_bits,
            } => {
                assert_eq!(username, "user.worker1");
                assert_eq!(job_id, "job-123");
                assert_eq!(extra_nonce2, vec![0u8; 4]);
                assert_eq!(ntime, 0x69b33a70);
                assert_eq!(nonce, 0x12345678);
                assert_eq!(version_bits, None);
            }
            _ => panic!("expected SubmitShare"),
        }
    }

    #[test]
    fn test_submit_version_bits_parsed_when_present() {
        let req = Sv1Request {
            id: Some(serde_json::json!(1)),
            method: "mining.submit".to_string(),
            params: Some(serde_json::json!([
                "user.worker1",
                "job-123",
                "00000000",
                "69b33a70",
                "12345678",
                "00002000"
            ])),
        };
        let cmd = map_request_to_command(&req).unwrap().unwrap();
        match cmd {
            Sv1DomainCommand::SubmitShare { version_bits, .. } => {
                assert_eq!(version_bits, Some(0x00002000));
            }
            _ => panic!("expected SubmitShare"),
        }
    }

    #[test]
    fn test_submit_malformed_ntime_rejected() {
        let req = Sv1Request {
            id: None,
            method: "mining.submit".to_string(),
            params: Some(serde_json::json!([
                "user.worker1",
                "job-123",
                "00000000",
                "zz",
                "12345678"
            ])),
        };
        let err = map_request_to_command(&req).unwrap_err();
        assert!(err.contains("ntime"));
    }

    #[test]
    fn test_submit_too_few_params_rejected() {
        let req = Sv1Request {
            id: None,
            method: "mining.submit".to_string(),
            params: Some(serde_json::json!(["user.worker1", "job-123"])),
        };
        let err = map_request_to_command(&req).unwrap_err();
        assert!(err.contains("5 params"));
    }

    #[test]
    fn test_submit_ntime_wrong_length_rejected() {
        let req = Sv1Request {
            id: None,
            method: "mining.submit".to_string(),
            params: Some(serde_json::json!([
                "user.worker1",
                "job-123",
                "00000000",
                "123",
                "12345678"
            ])),
        };
        let err = map_request_to_command(&req).unwrap_err();
        assert!(err.contains("ntime"));
        assert!(err.contains("8 hex chars"));
    }

    #[test]
    fn test_submit_nonce_wrong_length_rejected() {
        let req = Sv1Request {
            id: None,
            method: "mining.submit".to_string(),
            params: Some(serde_json::json!([
                "user.worker1",
                "job-123",
                "00000000",
                "69b33a70",
                "123"
            ])),
        };
        let err = map_request_to_command(&req).unwrap_err();
        assert!(err.contains("nonce"));
        assert!(err.contains("8 hex chars"));
    }
}
