use crate::{bridge_block_template, AzcoinJobBridge, BridgeError};
use coin_azcoin::{build_solved_block_header, AzcoinShareValidator, BlockTemplate};
use pool_core::{ShareResult, ShareSubmission, ShareValidator, WorkerIdentity};
use std::error::Error;
use std::fmt;

/// Structural compatibility result for the current AZCoin share-validation path.
/// This protects the existing `build_solved_block_header` / `AzcoinShareValidator`
/// flow by ensuring the bridged `pool_core::Job` still carries the fields that path reads.
pub struct ShareFlowCompatibility {
    pub bridge: AzcoinJobBridge,
    pub solved_header: Vec<u8>,
    pub validation_result: ShareResult,
}

#[derive(Debug)]
pub enum ShareFlowCheckError {
    Bridge(BridgeError),
    HeaderBuild(String),
}

impl fmt::Display for ShareFlowCheckError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ShareFlowCheckError::Bridge(error) => write!(f, "bridge error: {}", error),
            ShareFlowCheckError::HeaderBuild(error) => write!(f, "header build error: {}", error),
        }
    }
}

impl Error for ShareFlowCheckError {}

impl From<BridgeError> for ShareFlowCheckError {
    fn from(value: BridgeError) -> Self {
        ShareFlowCheckError::Bridge(value)
    }
}

/// Check that a bridged template still feeds the current AZCoin share path.
///
/// Verified fields:
/// - `Job.prev_hash`
/// - `Job.version`
/// - `Job.nbits`
/// - `Job.coinbase_part1` / `Job.coinbase_part2`
/// - `Job.merkle_branch`
/// - `Job.block_assembly`
///
/// Still outside this scaffold boundary:
/// - session-derived extranonce sizing/negotiation
/// - real miner share acceptance at runtime
/// - pool job registry / protocol dispatch
pub fn check_share_flow_compatibility(
    template: &BlockTemplate,
    payout_script_pubkey: &[u8],
) -> Result<ShareFlowCompatibility, ShareFlowCheckError> {
    let bridge = bridge_block_template(template, payout_script_pubkey)?;
    let share = ShareSubmission {
        job_id: bridge.job.job_id.clone(),
        worker: WorkerIdentity::new("compat.worker"),
        extra_nonce2: vec![0, 0, 0, 0],
        ntime: bridge.job.ntime,
        nonce: 0,
        validation_context: None,
    };
    let extranonce1 = [0u8; 4];

    let solved_header = build_solved_block_header(&bridge.job, &share, &extranonce1)
        .map_err(ShareFlowCheckError::HeaderBuild)?;
    let validation_result =
        AzcoinShareValidator::new().validate_share(&bridge.job, &share, &extranonce1, 1);

    Ok(ShareFlowCompatibility {
        bridge,
        solved_header,
        validation_result,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use coin_azcoin::{BlockTemplate, TransactionEntry};

    fn fixture_payout_script() -> Vec<u8> {
        vec![
            0x76, 0xa9, 0x14, 0x00, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99,
            0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff, 0x00, 0x11, 0x22, 0x33, 0x88, 0xac,
        ]
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
            coinbasevalue: 5_000_000_000,
            coinbaseaux: None,
            default_witness_commitment: None,
        }
    }

    #[test]
    fn test_bridged_job_preserves_share_validation_inputs() {
        let compatibility =
            check_share_flow_compatibility(&fixture_template(), &fixture_payout_script()).unwrap();

        assert_eq!(
            compatibility.bridge.canonical.prev_hash.as_slice(),
            &compatibility.bridge.job.prev_hash
        );
        assert_eq!(
            compatibility.bridge.canonical.version,
            compatibility.bridge.job.version
        );
        assert_eq!(compatibility.bridge.canonical.nbits, compatibility.bridge.job.nbits);
        assert_eq!(compatibility.bridge.canonical.ntime, compatibility.bridge.job.ntime);
        assert!(!compatibility.bridge.job.coinbase_part1.is_empty());
        assert!(!compatibility.bridge.job.coinbase_part2.is_empty());
        assert_eq!(compatibility.bridge.job.merkle_branch.len(), 1);
        assert_eq!(compatibility.solved_header.len(), 80);
        assert!(matches!(
            compatibility.validation_result,
            ShareResult::Accepted | ShareResult::Block | ShareResult::LowDifficulty { .. }
        ));
        assert_eq!(
            compatibility
                .bridge
                .job
                .block_assembly
                .as_ref()
                .map(|assembly| assembly.height),
            Some(100)
        );
        assert_eq!(
            compatibility
                .bridge
                .job
                .block_assembly
                .as_ref()
                .map(|assembly| assembly.coinbase_value),
            Some(5_000_000_000)
        );
    }
}
