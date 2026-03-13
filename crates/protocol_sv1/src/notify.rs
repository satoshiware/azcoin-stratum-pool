//! Stratum V1 mining.notify message builder. Wire format stays in protocol_sv1.

use pool_core::Job;
use serde_json::json;

/// Build a mining.notify JSON-RPC notification from an internal Job.
/// SV1 params: job_id, prev_hash, coinb1, coinb2, merkle_branch, version, nbits, ntime, clean_jobs.
pub fn build_mining_notify(job: &Job) -> serde_json::Value {
    let prev_hash_hex = hex_encode_reversed(&job.prev_hash);
    let coinb1_hex = hex::encode(&job.coinbase_part1);
    let coinb2_hex = hex::encode(&job.coinbase_part2);
    let merkle_hex: Vec<String> = job.merkle_branch.iter().map(hex_encode_reversed).collect();
    let version_hex = format!("{:08x}", job.version);
    let nbits_hex = format!("{:08x}", job.nbits);
    let ntime_hex = format!("{:08x}", job.ntime);

    json!({
        "method": "mining.notify",
        "params": [
            job.job_id,
            prev_hash_hex,
            coinb1_hex,
            coinb2_hex,
            merkle_hex,
            version_hex,
            nbits_hex,
            ntime_hex,
            job.clean_jobs
        ]
    })
}

fn hex_encode_reversed(bytes: &[u8; 32]) -> String {
    let reversed: Vec<u8> = bytes.iter().rev().copied().collect();
    hex::encode(reversed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use pool_core::Job;

    #[test]
    fn test_job_to_notify_mapping() {
        let mut prev_hash = [0u8; 32];
        prev_hash[31] = 0xab;
        let job = Job {
            job_id: "live-job-123".to_string(),
            prev_hash,
            coinbase_part1: vec![0x01, 0x02],
            coinbase_part2: vec![0xff, 0xfe],
            merkle_branch: vec![],
            version: 0x20000000,
            nbits: 0x1d00ffff,
            ntime: 0x69b33a70,
            clean_jobs: true,
        };

        let notify = build_mining_notify(&job);
        assert_eq!(notify["method"], "mining.notify");
        let params = notify["params"].as_array().unwrap();
        assert_eq!(params.len(), 9);
        assert_eq!(params[0], "live-job-123");
        assert_eq!(params[1], "ab00000000000000000000000000000000000000000000000000000000000000");
        assert_eq!(params[2], "0102");
        assert_eq!(params[3], "fffe");
        assert_eq!(params[4], serde_json::json!([]));
        assert_eq!(params[5], "20000000");
        assert_eq!(params[6], "1d00ffff");
        assert_eq!(params[7], "69b33a70");
        assert_eq!(params[8], true);
    }

    #[test]
    fn test_job_to_notify_placeholder() {
        let job = Job::placeholder();
        let notify = build_mining_notify(&job);
        assert_eq!(notify["method"], "mining.notify");
        let params = notify["params"].as_array().unwrap();
        assert_eq!(params[0], "0");
        assert_eq!(params[8], true);
    }
}
