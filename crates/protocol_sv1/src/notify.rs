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
