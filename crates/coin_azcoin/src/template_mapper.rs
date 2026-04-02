//! Maps daemon block template to pool_core::Job.
//! TODO: AZCOIN-specific coinbase and merkle construction may need refinement.

use crate::coinbase_builder::{build_coinbase_transaction, serialize_no_witness, CoinbaseBuildInputs};
use crate::daemon::BlockTemplate;
use common::PoolError;
use pool_core::{BlockAssemblyData, Job};

const SV1_EXTRANONCE_PLACEHOLDER: [u8; 8] = [0xfa, 0xce, 0xb0, 0x0c, 0xde, 0xad, 0xbe, 0xef];

/// Normalize timestamp to u32 for pool_core::Job. Same for RPC and API.
fn ntime_to_u32(curtime: u64) -> u32 {
    curtime as u32
}

/// Convert daemon block template to protocol-agnostic Job.
pub fn template_to_job(
    template: &BlockTemplate,
    payout_script_pubkey: &[u8],
) -> Result<Job, PoolError> {
    let prev_hash = decode_prev_hash(&template.previousblockhash)?;
    let nbits = decode_bits(&template.bits)?;
    let merkle_branch = build_merkle_branch(template);
    let coinbase_aux_flags = decode_coinbase_aux_flags(template)?;
    let default_witness_commitment = decode_default_witness_commitment(template)?;
    let template_transactions = decode_template_transactions(template)?;
    let (coinbase_part1, coinbase_part2) = build_coinbase_parts(
        template,
        payout_script_pubkey,
        coinbase_aux_flags.as_deref(),
        default_witness_commitment.as_deref(),
    )?;

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
            template_transactions,
            default_witness_commitment,
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
    let Some(flags) = template
        .coinbaseaux
        .as_ref()
        .and_then(|aux| aux.flags.as_ref())
    else {
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

fn decode_default_witness_commitment(template: &BlockTemplate) -> Result<Option<Vec<u8>>, PoolError> {
    let Some(commitment) = template.default_witness_commitment.as_ref() else {
        return Ok(None);
    };
    let commitment = commitment.trim();
    if commitment.is_empty() {
        return Ok(None);
    }

    hex::decode(commitment)
        .map(Some)
        .map_err(|e| PoolError::Daemon(format!("default_witness_commitment hex: {}", e)))
}

fn decode_template_transactions(template: &BlockTemplate) -> Result<Vec<Vec<u8>>, PoolError> {
    template
        .transactions
        .iter()
        .enumerate()
        .map(|(index, tx)| {
            hex::decode(&tx.data)
                .map_err(|e| PoolError::Daemon(format!("transaction {} data hex: {}", index, e)))
        })
        .collect()
}

/// Build merkle branch from transaction txids (non-witness hashes).
/// Prefers txid over hash because the block header merkle root uses txids.
fn build_merkle_branch(template: &BlockTemplate) -> Vec<[u8; 32]> {
    let mut branch = Vec::with_capacity(template.transactions.len());
    for tx in &template.transactions {
        if let Some(h) = tx.txid.as_ref().or(tx.hash.as_ref()) {
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
/// Parts are the NON-WITNESS (legacy) serialization so that miners hashing
/// part1 + extranonce1 + extranonce2 + part2 compute the txid, which is what
/// the block header merkle root must be built from.
fn build_coinbase_parts(
    template: &BlockTemplate,
    payout_script_pubkey: &[u8],
    coinbase_aux_flags: Option<&[u8]>,
    default_witness_commitment: Option<&[u8]>,
) -> Result<(Vec<u8>, Vec<u8>), PoolError> {
    let mut flags_with_extranonce = coinbase_aux_flags.unwrap_or(&[]).to_vec();
    flags_with_extranonce.extend_from_slice(&SV1_EXTRANONCE_PLACEHOLDER);

    let coinbase_tx_bytes = build_coinbase_transaction(&CoinbaseBuildInputs {
        height: template.height,
        coinbase_value: template.coinbasevalue,
        payout_script_pubkey: payout_script_pubkey.to_vec(),
        coinbase_aux_flags: Some(flags_with_extranonce),
        default_witness_commitment: default_witness_commitment.map(|commitment| commitment.to_vec()),
    })?;

    // Re-serialize as non-witness so miners hash the txid serialization.
    let coinbase_tx: bitcoin::Transaction =
        bitcoin::consensus::deserialize(&coinbase_tx_bytes)
            .map_err(|e| PoolError::Internal(format!("coinbase deserialize: {}", e)))?;
    let coinbase_no_witness = serialize_no_witness(&coinbase_tx);

    let split_at = coinbase_no_witness
        .windows(SV1_EXTRANONCE_PLACEHOLDER.len())
        .position(|window| window == SV1_EXTRANONCE_PLACEHOLDER)
        .ok_or_else(|| {
            PoolError::Internal("coinbase transaction missing SV1 extranonce placeholder".into())
        })?;

    Ok((
        coinbase_no_witness[..split_at].to_vec(),
        coinbase_no_witness[split_at + SV1_EXTRANONCE_PLACEHOLDER.len()..].to_vec(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::daemon::{BlockTemplate, CoinbaseAux, TransactionEntry};
    use bitcoin::consensus::deserialize;
    use bitcoin::Transaction;

    fn fixture_payout_script() -> Vec<u8> {
        hex::decode("76a91400112233445566778899aabbccddeeff0011223388ac").unwrap()
    }

    fn fixture_template() -> BlockTemplate {
        BlockTemplate {
            version: 0x20000000,
            previousblockhash: "0000000000000000000000000000000000000000000000000000000000000001"
                .to_string(),
            bits: "1d00ffff".to_string(),
            curtime: 1700000000,
            height: 100,
            transactions: vec![TransactionEntry {
                data: "deadbeef".to_string(),
                txid: Some("a".repeat(64)),
                hash: Some("b".repeat(64)),
            }],
            coinbasevalue: 5000000000,
            coinbaseaux: None,
            default_witness_commitment: None,
        }
    }

    #[test]
    fn test_template_to_job_maps_fields() {
        let template = fixture_template();
        let job = template_to_job(&template, &fixture_payout_script()).unwrap();

        assert_eq!(job.job_id, "100");
        assert_eq!(job.version, 0x20000000);
        assert_eq!(job.nbits, 0x1d00ffff);
        assert_eq!(job.ntime, 1700000000);
        assert!(job.clean_jobs);

        // prev_hash: hex 00..01 decodes to [0,0,...,0,1], reversed -> [1,0,...,0]
        assert_eq!(job.prev_hash[0], 0x01);
        assert_eq!(job.prev_hash[31], 0x00);

        assert!(!job.coinbase_part1.is_empty());
        assert!(!job.coinbase_part2.is_empty());
        assert_ne!(job.coinbase_part2, vec![0xff, 0xff, 0xff, 0xff]);
        assert_eq!(
            job.block_assembly
                .as_ref()
                .and_then(|assembly| assembly.coinbase_aux_flags.as_ref()),
            None
        );
        assert_eq!(
            job.block_assembly.unwrap().template_transactions,
            vec![hex::decode("deadbeef").unwrap()]
        );
    }

    #[test]
    fn test_template_to_job_empty_transactions() {
        let mut template = fixture_template();
        template.transactions.clear();
        let job = template_to_job(&template, &fixture_payout_script()).unwrap();
        assert!(job.merkle_branch.is_empty());
    }

    #[test]
    fn test_template_to_job_invalid_prev_hash_fails() {
        let mut template = fixture_template();
        template.previousblockhash = "zz".to_string();
        assert!(template_to_job(&template, &fixture_payout_script()).is_err());
    }

    #[test]
    fn test_template_to_job_invalid_bits_fails() {
        let mut template = fixture_template();
        template.bits = "1d00ff".to_string(); // 3 bytes, not 4
        assert!(template_to_job(&template, &fixture_payout_script()).is_err());
    }

    #[test]
    fn test_template_to_job_preserves_coinbase_aux_flags() {
        let mut template = fixture_template();
        template.coinbaseaux = Some(CoinbaseAux {
            flags: Some("deadbeef".to_string()),
        });

        let job = template_to_job(&template, &fixture_payout_script()).unwrap();

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

        let err = template_to_job(&template, &fixture_payout_script()).unwrap_err();
        assert!(matches!(err, PoolError::Daemon(_)));
        assert!(err.to_string().contains("coinbaseaux.flags hex"));
    }

    #[test]
    fn test_template_to_job_coinbase_parts_round_trip_with_extranonce() {
        let mut template = fixture_template();
        template.coinbaseaux = Some(CoinbaseAux {
            flags: Some("deadbeef".to_string()),
        });
        let payout_script = fixture_payout_script();
        let job = template_to_job(&template, &payout_script).unwrap();
        let extranonce1 = [0xaa, 0xbb, 0xcc, 0xdd];
        let extranonce2 = [0x11, 0x22, 0x33, 0x44];

        let mut coinbase = job.coinbase_part1.clone();
        coinbase.extend_from_slice(&extranonce1);
        coinbase.extend_from_slice(&extranonce2);
        coinbase.extend_from_slice(&job.coinbase_part2);

        let tx: Transaction = deserialize(&coinbase).unwrap();
        let script_sig = tx.input[0].script_sig.as_bytes();

        assert!(script_sig.windows(4).any(|window| window == [0xde, 0xad, 0xbe, 0xef]));
        assert!(script_sig.windows(8).any(|window| window == [
            0xaa, 0xbb, 0xcc, 0xdd, 0x11, 0x22, 0x33, 0x44
        ]));
        assert_eq!(tx.output[0].value.to_sat(), template.coinbasevalue);
        assert_eq!(tx.output[0].script_pubkey.as_bytes(), payout_script.as_slice());
    }

    #[test]
    fn test_coinbase_parts_are_non_witness_when_witness_commitment_present() {
        let mut template = fixture_template();
        template.default_witness_commitment = Some(
            "6a24aa21a9ed11223344556677889900aabbccddeeff00112233445566778899".to_string(),
        );
        let job = template_to_job(&template, &fixture_payout_script()).unwrap();

        let mut coinbase = job.coinbase_part1.clone();
        coinbase.extend_from_slice(&[0xaa; 4]); // extranonce1
        coinbase.extend_from_slice(&[0xbb; 4]); // extranonce2
        coinbase.extend_from_slice(&job.coinbase_part2);

        // Byte 4 must be input count (0x01), not segwit marker (0x00)
        assert_eq!(coinbase[4], 0x01, "coinbase parts must be non-witness serialization");

        // Must still deserialize as a valid transaction
        let tx: Transaction = deserialize(&coinbase).unwrap();
        // Witness commitment output must be present in the outputs
        assert_eq!(tx.output.len(), 2);
        // But deserialized from non-witness, witness field is empty
        assert!(tx.input[0].witness.is_empty());
    }

    #[test]
    fn test_build_merkle_branch_prefers_txid_over_hash() {
        let mut template = fixture_template();
        template.transactions = vec![TransactionEntry {
            data: "deadbeef".to_string(),
            txid: Some("aa".repeat(32)),
            hash: Some("bb".repeat(32)),
        }];

        let branch = build_merkle_branch(&template);
        assert_eq!(branch.len(), 1);
        // txid "aa..aa" decoded + reversed
        let mut expected = [0xaa; 32];
        expected.reverse();
        assert_eq!(branch[0], expected);
    }
}
