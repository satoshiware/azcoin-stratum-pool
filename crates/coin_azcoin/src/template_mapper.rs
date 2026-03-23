//! Maps daemon block template to pool_core::Job.
//! TODO: AZCOIN-specific coinbase and merkle construction may need refinement.

use crate::daemon::BlockTemplate;
use common::PoolError;
use pool_core::{BlockAssemblyData, Job};

/// Normalize timestamp to u32 for pool_core::Job. Same for RPC and API.
fn ntime_to_u32(curtime: u64) -> u32 {
    curtime as u32
}

/// Convert daemon block template to protocol-agnostic Job.
pub fn template_to_job(template: &BlockTemplate) -> Result<Job, PoolError> {
    let prev_hash = decode_prev_hash(&template.previousblockhash)?;
    let nbits = decode_bits(&template.bits)?;
    let merkle_branch = build_merkle_branch(template);
    let coinbase_aux_flags = decode_coinbase_aux_flags(template)?;

    // TODO: AZCOIN-specific coinbase construction. Pool must build coinbase from
    // coinbasevalue, height, extranonce, etc. For now use minimal placeholder parts.
    let (coinbase_part1, coinbase_part2) = build_coinbase_parts(template);

    Ok(Job {
        job_id: template.height.to_string(),
        prev_hash,
        coinbase_part1,
        coinbase_part2,
        merkle_branch,
        version: template.version,
        nbits,
        ntime: ntime_to_u32(template.curtime),
        clean_jobs: true,
        block_assembly: Some(BlockAssemblyData {
            height: template.height,
            coinbase_value: template.coinbasevalue,
            coinbase_aux_flags,
            template_transactions: Vec::new(),
            default_witness_commitment: None,
        }),
    })
}

pub(crate) fn decode_prev_hash(hex: &str) -> Result<[u8; 32], PoolError> {
    let bytes = hex::decode(hex).map_err(|e| PoolError::Daemon(format!("prev_hash hex: {}", e)))?;
    if bytes.len() != 32 {
        return Err(PoolError::Daemon(format!(
            "prev_hash length {} != 32",
            bytes.len()
        )));
    }
    let mut arr = [0u8; 32];
    arr.copy_from_slice(&bytes);
    arr.reverse(); // block header stores hashes in reverse byte order
    Ok(arr)
}

pub(crate) fn decode_bits(hex: &str) -> Result<u32, PoolError> {
    let bytes = hex::decode(hex).map_err(|e| PoolError::Daemon(format!("bits hex: {}", e)))?;
    if bytes.len() != 4 {
        return Err(PoolError::Daemon(format!(
            "bits length {} != 4",
            bytes.len()
        )));
    }
    Ok(u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

fn decode_coinbase_aux_flags(template: &BlockTemplate) -> Result<Option<Vec<u8>>, PoolError> {
    let Some(flags) = template.coinbaseaux.as_ref().and_then(|aux| aux.flags.as_ref()) else {
        return Ok(None);
    };
    let flags = flags.trim();
    if flags.is_empty() {
        return Ok(None);
    }

    hex::decode(flags)
        .map(Some)
        .map_err(|e| PoolError::Daemon(format!("coinbaseaux.flags hex: {}", e)))
}

/// Build merkle branch from transaction hashes.
/// TODO: AZCOIN merkle format may differ. Use txid/hash from template when available.
fn build_merkle_branch(template: &BlockTemplate) -> Vec<[u8; 32]> {
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

/// Build coinbase part1 (before extranonce) and part2 (after extranonce).
/// TODO: AZCOIN-specific. Must include version, height, coinbasevalue, pool data.
fn build_coinbase_parts(template: &BlockTemplate) -> (Vec<u8>, Vec<u8>) {
    // Minimal placeholder: version + height in part1, coinbasevalue placeholder in part2.
    let mut part1 = vec![0x01, 0x00, 0x00, 0x00, 0x00]; // version
    part1.extend_from_slice(&template.height.to_le_bytes());
    let part2 = vec![0xff, 0xff, 0xff, 0xff]; // placeholder after extranonce
    (part1, part2)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::daemon::{BlockTemplate, CoinbaseAux, TransactionEntry};

    fn fixture_template() -> BlockTemplate {
        BlockTemplate {
            version: 0x20000000,
            previousblockhash: "0000000000000000000000000000000000000000000000000000000000000001"
                .to_string(),
            bits: "1d00ffff".to_string(),
            curtime: 1700000000,
            height: 100,
            transactions: vec![TransactionEntry {
                data: "0100000001...".to_string(),
                txid: Some("a".repeat(64)),
                hash: Some("b".repeat(64)),
            }],
            coinbasevalue: 5000000000,
            coinbaseaux: None,
        }
    }

    #[test]
    fn test_template_to_job_maps_fields() {
        let template = fixture_template();
        let job = template_to_job(&template).unwrap();

        assert_eq!(job.job_id, "100");
        assert_eq!(job.version, 0x20000000);
        assert_eq!(job.nbits, 0x1d00ffff);
        assert_eq!(job.ntime, 1700000000);
        assert!(job.clean_jobs);

        // prev_hash: hex 00..01 decodes to [0,0,...,0,1], reversed -> [1,0,...,0]
        assert_eq!(job.prev_hash[0], 0x01);
        assert_eq!(job.prev_hash[31], 0x00);

        // coinbase parts include height
        assert!(job.coinbase_part1.len() >= 5 + 8);
        assert_eq!(job.coinbase_part2, vec![0xff, 0xff, 0xff, 0xff]);
        assert_eq!(
            job.block_assembly
                .as_ref()
                .and_then(|assembly| assembly.coinbase_aux_flags.as_ref()),
            None
        );
    }

    #[test]
    fn test_template_to_job_empty_transactions() {
        let mut template = fixture_template();
        template.transactions.clear();
        let job = template_to_job(&template).unwrap();
        assert!(job.merkle_branch.is_empty());
    }

    #[test]
    fn test_template_to_job_invalid_prev_hash_fails() {
        let mut template = fixture_template();
        template.previousblockhash = "zz".to_string();
        assert!(template_to_job(&template).is_err());
    }

    #[test]
    fn test_template_to_job_invalid_bits_fails() {
        let mut template = fixture_template();
        template.bits = "1d00ff".to_string(); // 3 bytes, not 4
        assert!(template_to_job(&template).is_err());
    }

    #[test]
    fn test_template_to_job_preserves_coinbase_aux_flags() {
        let mut template = fixture_template();
        template.coinbaseaux = Some(CoinbaseAux {
            flags: Some("deadbeef".to_string()),
        });

        let job = template_to_job(&template).unwrap();

        assert_eq!(
            job.block_assembly.unwrap().coinbase_aux_flags,
            Some(vec![0xde, 0xad, 0xbe, 0xef])
        );
    }

    #[test]
    fn test_template_to_job_invalid_coinbase_aux_flags_fails() {
        let mut template = fixture_template();
        template.coinbaseaux = Some(CoinbaseAux {
            flags: Some("abc".to_string()),
        });

        let err = template_to_job(&template).unwrap_err();
        assert!(matches!(err, PoolError::Daemon(_)));
        assert!(err.to_string().contains("coinbaseaux.flags hex"));
    }
}
