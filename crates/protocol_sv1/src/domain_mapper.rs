//! Maps Stratum V1 requests into internal domain commands.
//! Keeps SV1 wire format separate from pool_core domain types.

use crate::messages::{Sv1DomainCommand, Sv1Request};
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
    let extensions = match req.params.as_ref() {
        None => Vec::new(),
        Some(params) => {
            let params = params
                .as_array()
                .ok_or("mining.configure requires params array")?;
            match params.first() {
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
            }
        }
    };

    Ok(Sv1DomainCommand::Configure { extensions })
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

    Ok(Sv1DomainCommand::SubmitShare {
        username,
        job_id,
        extra_nonce2,
        ntime,
        nonce,
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
            Sv1DomainCommand::Configure { extensions } => {
                assert_eq!(
                    extensions,
                    vec![
                        "version-rolling".to_string(),
                        "minimum-difficulty".to_string(),
                        "subscribe-extranonce".to_string()
                    ]
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
            Sv1DomainCommand::Configure { extensions } => assert!(extensions.is_empty()),
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
            } => {
                assert_eq!(username, "user.worker1");
                assert_eq!(job_id, "job-123");
                assert_eq!(extra_nonce2, vec![0u8; 4]);
                assert_eq!(ntime, 0x69b33a70);
                assert_eq!(nonce, 0x12345678);
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
