//! Maps node API template DTO to pool_core::Job.
//! ntime: API returns hex string, normalized to u32 for consistent internal representation.

use crate::node_api::NodeApiTemplate;
use common::PoolError;
use pool_core::Job;

use crate::template_mapper::{decode_bits, decode_prev_hash};

/// Convert node API template to protocol-agnostic Job.
pub fn api_template_to_job(template: &NodeApiTemplate) -> Result<Job, PoolError> {
    let prev_hash = decode_prev_hash(&template.prev_hash)?;
    let nbits = decode_bits(&template.nbits)?;
    let ntime = ntime_hex_to_u32(&template.ntime)?;

    // API does not provide transactions or coinbase. Use placeholders.
    let merkle_branch: Vec<[u8; 32]> = vec![];
    let (coinbase_part1, coinbase_part2) = build_coinbase_parts(template);

    Ok(Job {
        job_id: template.job_id.clone(),
        prev_hash,
        coinbase_part1,
        coinbase_part2,
        merkle_branch,
        version: template.version,
        nbits,
        ntime,
        clean_jobs: template.clean_jobs,
    })
}

/// Parse ntime hex string (e.g. "69b33a70") to u32. Block header uses little-endian.
fn ntime_hex_to_u32(s: &str) -> Result<u32, PoolError> {
    let bytes = hex::decode(s.trim()).map_err(|e| PoolError::Daemon(format!("ntime hex: {}", e)))?;
    if bytes.len() != 4 {
        return Err(PoolError::Daemon(format!("ntime length {} != 4", bytes.len())));
    }
    Ok(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

fn build_coinbase_parts(template: &NodeApiTemplate) -> (Vec<u8>, Vec<u8>) {
    let mut part1 = vec![0x01, 0x00, 0x00, 0x00, 0x00];
    part1.extend_from_slice(&template.height.to_le_bytes());
    let part2 = vec![0xff, 0xff, 0xff, 0xff];
    (part1, part2)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_api_template() -> NodeApiTemplate {
        NodeApiTemplate {
            job_id: "test-job-123".to_string(),
            prev_hash: "0000000000000000000000000000000000000000000000000000000000000001"
                .to_string(),
            version: 0x20000000,
            nbits: "1d00ffff".to_string(),
            ntime: "65900000".to_string(), // hex for ~1700000000 in LE
            clean_jobs: true,
            height: 200,
        }
    }

    #[test]
    fn test_api_template_to_job_maps_fields() {
        let template = fixture_api_template();
        let job = api_template_to_job(&template).unwrap();

        assert_eq!(job.job_id, "test-job-123");
        assert_eq!(job.version, 0x20000000);
        assert_eq!(job.nbits, 0x1d00ffff);
        assert!(job.clean_jobs);

        assert_eq!(job.prev_hash[0], 0x01);
        assert_eq!(job.prev_hash[31], 0x00);

        assert!(job.coinbase_part1.len() >= 5 + 8);
        assert_eq!(job.coinbase_part2, vec![0xff, 0xff, 0xff, 0xff]);
    }

    #[test]
    fn test_api_template_to_job_real_payload() {
        let json = r#"{
            "job_id":"c68fdce62b92e2d8",
            "prev_hash":"0000000000000000c589462bc769be8b4a12fddd736d5bd5e47966e10421222b",
            "version":536870912,
            "nbits":"1a020e7c",
            "ntime":"69b33a70",
            "clean_jobs":true,
            "height":808523
        }"#;
        let template: NodeApiTemplate = serde_json::from_str(json).unwrap();
        let job = api_template_to_job(&template).unwrap();
        assert_eq!(job.job_id, "c68fdce62b92e2d8");
        assert_eq!(job.version, 536870912);
        assert!(job.clean_jobs);
    }

    #[test]
    fn test_api_template_to_job_invalid_prev_hash_fails() {
        let mut template = fixture_api_template();
        template.prev_hash = "zz".to_string();
        assert!(api_template_to_job(&template).is_err());
    }

    #[test]
    fn test_api_template_to_job_invalid_ntime_fails() {
        let mut template = fixture_api_template();
        template.ntime = "zz".to_string();
        assert!(api_template_to_job(&template).is_err());
    }
}
