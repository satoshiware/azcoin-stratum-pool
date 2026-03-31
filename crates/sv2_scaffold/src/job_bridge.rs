use crate::{from_block_template, AdapterError, AzcoinJobTemplate};
use coin_azcoin::{template_to_job, BlockTemplate};
use pool_core::Job;
use std::error::Error;
use std::fmt;

/// Compatibility output for the current `pool_core::Job` construction path.
/// This lets SV2-facing code inspect the normalized template while still reusing
/// the existing V1/AZCoin job builder unchanged.
pub struct AzcoinJobBridge {
    pub canonical: AzcoinJobTemplate,
    pub job: Job,
}

#[derive(Debug)]
pub enum BridgeError {
    Adapter(AdapterError),
    JobConstruction(String),
}

impl fmt::Display for BridgeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BridgeError::Adapter(error) => write!(f, "template adapter error: {}", error),
            BridgeError::JobConstruction(error) => write!(f, "job construction error: {}", error),
        }
    }
}

impl Error for BridgeError {}

impl From<AdapterError> for BridgeError {
    fn from(value: AdapterError) -> Self {
        BridgeError::Adapter(value)
    }
}

/// Bridge a daemon `BlockTemplate` into the normalized adapter form plus the
/// existing `pool_core::Job` used by the current mining stack.
///
/// Missing fields before full SV2 integration:
/// - coinbase parts still depend on the existing payout script input
/// - merkle/coinbase assembly still comes from `coin_azcoin::template_to_job`
///
/// Endianness note:
/// - `canonical.prev_hash` carries forward the same reversed block-header byte
///   order used by the current AZCoin template mapper.
pub fn bridge_block_template(
    template: &BlockTemplate,
    payout_script_pubkey: &[u8],
) -> Result<AzcoinJobBridge, BridgeError> {
    let canonical = from_block_template(template)?;
    let job = template_to_job(template, payout_script_pubkey)
        .map_err(|error| BridgeError::JobConstruction(error.to_string()))?;

    Ok(AzcoinJobBridge { canonical, job })
}

#[cfg(test)]
mod tests {
    use super::*;
    use coin_azcoin::TransactionEntry;

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
    fn test_bridge_block_template_matches_existing_job_path() {
        let template = fixture_template();

        let bridge = bridge_block_template(&template, &fixture_payout_script()).unwrap();

        assert_eq!(
            bridge.canonical.job_id,
            "100:0000000000000000000000000000000000000000000000000000000000000001:1700000000"
        );
        assert_eq!(bridge.canonical.version, bridge.job.version);
        assert_eq!(bridge.canonical.nbits, bridge.job.nbits);
        assert_eq!(bridge.canonical.ntime, bridge.job.ntime);
        assert_eq!(bridge.canonical.prev_hash.as_slice(), &bridge.job.prev_hash);
        assert_eq!(bridge.job.job_id, "100");
        assert!(bridge.job.clean_jobs);
        assert_eq!(
            bridge.job
                .block_assembly
                .as_ref()
                .map(|assembly| assembly.height),
            Some(100)
        );
        assert_eq!(
            bridge.job
                .block_assembly
                .as_ref()
                .map(|assembly| assembly.coinbase_value),
            Some(5_000_000_000)
        );
    }
}
