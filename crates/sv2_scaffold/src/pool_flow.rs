use crate::{
    bridge_block_template, AggregationEngine, AzcoinJobBridge, BridgeError, MinerId, MinerStats,
    ShareRecord,
};
use coin_azcoin::BlockTemplate;
use std::error::Error;
use std::fmt;

/// Pool-only internal compatibility flow for future SV2 integration work.
///
/// This is intentionally limited to:
/// - template normalization and job bridging
/// - in-memory pooled-share/accounting recording
///
/// It does not cover:
/// - network ingress
/// - real share submission
/// - translator wiring
/// - SV2 protocol messages
/// - persistence
/// - payout math
pub struct PoolFlowResult {
    pub bridge: AzcoinJobBridge,
    pub miner: MinerId,
    pub stats: MinerStats,
}

#[derive(Debug)]
pub enum PoolFlowError {
    Bridge(BridgeError),
    MinerStatsMissing,
}

impl fmt::Display for PoolFlowError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PoolFlowError::Bridge(error) => write!(f, "bridge error: {}", error),
            PoolFlowError::MinerStatsMissing => {
                write!(f, "miner stats missing after share record")
            }
        }
    }
}

impl Error for PoolFlowError {}

impl From<BridgeError> for PoolFlowError {
    fn from(value: BridgeError) -> Self {
        PoolFlowError::Bridge(value)
    }
}

/// Run a minimal pooled-mining internal flow:
/// `BlockTemplate -> AzcoinJobTemplate -> pool_core::Job -> ShareRecord -> AggregationEngine`.
///
/// This protects future integration work by proving the scaffold can bridge the
/// current AZCoin template path into the pool-oriented accounting layer without
/// changing existing mining logic.
pub fn run_pool_flow(
    template: &BlockTemplate,
    payout_script_pubkey: &[u8],
    miner: MinerId,
    share: ShareRecord,
    engine: &mut AggregationEngine,
) -> Result<PoolFlowResult, PoolFlowError> {
    let bridge = bridge_block_template(template, payout_script_pubkey)?;
    engine.record_share(share);
    let stats = engine
        .get_stats(&miner)
        .cloned()
        .ok_or(PoolFlowError::MinerStatsMissing)?;

    Ok(PoolFlowResult {
        bridge,
        miner,
        stats,
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
    fn test_pool_flow_records_share_and_preserves_bridge_consistency() {
        let mut engine = AggregationEngine::new();
        let miner = MinerId {
            username: "alice".to_string(),
            worker: "rig1".to_string(),
        };
        let share = ShareRecord {
            miner: miner.clone(),
            difficulty: 32.0,
            timestamp: 123,
            accepted: true,
        };

        let result = run_pool_flow(
            &fixture_template(),
            &fixture_payout_script(),
            miner.clone(),
            share,
            &mut engine,
        )
        .unwrap();

        assert_eq!(result.miner.username, "alice");
        assert_eq!(result.miner.worker, "rig1");
        assert_eq!(result.stats.total_shares, 1);
        assert_eq!(result.stats.accepted_shares, 1);
        assert_eq!(result.stats.rejected_shares, 0);
        assert_eq!(result.stats.last_share_time, 123);
        assert_eq!(result.bridge.canonical.version, result.bridge.job.version);
        assert_eq!(result.bridge.canonical.nbits, result.bridge.job.nbits);
        assert_eq!(result.bridge.canonical.ntime, result.bridge.job.ntime);
        assert_eq!(
            result.bridge.canonical.prev_hash.as_slice(),
            &result.bridge.job.prev_hash
        );
        assert_eq!(result.bridge.job.job_id, "100");
        assert_eq!(
            result.bridge
                .job
                .block_assembly
                .as_ref()
                .map(|assembly| assembly.coinbase_value),
            Some(5_000_000_000)
        );
        assert_eq!(engine.get_stats(&miner), Some(&result.stats));
    }
}
