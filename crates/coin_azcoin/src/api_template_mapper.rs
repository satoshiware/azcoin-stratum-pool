//! Maps node API template DTO to pool_core::Job.
//! ntime normalized to u32 (same as RPC path) for consistent internal representation.

use crate::node_api::NodeApiTemplate;
use common::PoolError;
use pool_core::Job;

use crate::template_mapper::{decode_bits, decode_prev_hash};

/// Convert node API template to protocol-agnostic Job.
/// ntime: always u32 (Unix timestamp), same as RPC path.
pub fn api_template_to_job(template: &NodeApiTemplate) -> Result<Job, PoolError> {
    let prev_hash = decode_prev_hash(&template.previous_block_hash)?;
    let nbits = decode_bits(&template.bits)?;
    let merkle_branch = build_merkle_branch(template);
    let (coinbase_part1, coinbase_part2) = build_coinbase_parts(template);

    // Normalize ntime to u32. Both RPC and API use same representation.
    let ntime = ntime_to_u32(template.curtime);

    Ok(Job {
        job_id: template.height.to_string(),
        prev_hash,
        coinbase_part1,
        coinbase_part2,
        merkle_branch,
        version: template.version,
        nbits,
        ntime,
        clean_jobs: true,
    })
}

/// Normalize timestamp to u32 for pool_core::Job. Same for RPC and API.
fn ntime_to_u32(curtime: u64) -> u32 {
    curtime as u32
}

fn build_merkle_branch(template: &NodeApiTemplate) -> Vec<[u8; 32]> {
    let mut branch = Vec::with_capacity(template.transactions.len());
    for tx in &template.transactions {
        if let Some(h) = tx.hash.as_ref().or(tx.txid.as_ref()) {
            if let Ok(b) = hex::decode(h) {
                if b.len() == 32 {
                    let mut arr = [0u8; 32];
                    arr.copy_from_slice(&b);
                    arr.reverse();
                    branch.push(arr);
                }
            }
        }
    }
    branch
}

fn build_coinbase_parts(template: &NodeApiTemplate) -> (Vec<u8>, Vec<u8>) {
    let mut part1 = vec![0x01, 0x00, 0x00, 0x00, 0x00];
    part1.extend_from_slice(&template.height.to_le_bytes());
    let part2 = vec![0xff, 0xff, 0xff, 0xff];
    (part1, part2)
}
