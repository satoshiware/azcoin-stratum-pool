//! Maps Stratum V1 requests into internal domain commands.
//! Keeps SV1 wire format separate from pool_core domain types.

use crate::messages::{Sv1DomainCommand, Sv1Request};
use tracing::warn;

/// Parse SV1 request into domain command. Returns None if unknown method.
pub fn map_request_to_command(req: &Sv1Request) -> Option<Sv1DomainCommand> {
    match req.method.as_str() {
        "mining.subscribe" => Some(Sv1DomainCommand::Subscribe),
        "mining.authorize" => {
            let params = req.params.as_ref()?.as_array()?;
            let username = params.first()?.as_str()?.to_string();
            let password = params
                .get(1)
                .and_then(|p| p.as_str())
                .unwrap_or("")
                .to_string();
            Some(Sv1DomainCommand::Authorize { username, password })
        }
        "mining.submit" => {
            let params = req.params.as_ref()?.as_array()?;
            let username = params.first()?.as_str()?.to_string();
            let job_id = params.get(1)?.as_str()?.to_string();
            let extra_nonce2_hex = params.get(2)?.as_str()?;
            let ntime_hex = params.get(3)?.as_str()?;
            let nonce_hex = params.get(4)?.as_str()?;

            let extra_nonce2 = hex::decode(extra_nonce2_hex).ok()?;
            let ntime = u32::from_str_radix(ntime_hex, 16).ok()?;
            let nonce = u32::from_str_radix(nonce_hex, 16).ok()?;

            Some(Sv1DomainCommand::SubmitShare {
                username,
                job_id,
                extra_nonce2,
                ntime,
                nonce,
            })
        }
        _ => {
            warn!(method = %req.method, "unknown SV1 method");
            None
        }
    }
}
