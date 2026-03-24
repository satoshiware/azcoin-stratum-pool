#![allow(dead_code)]

use bitcoin::consensus::deserialize;
use bitcoin::consensus::encode::{Encodable, VarInt};
use bitcoin::Transaction;
use common::PoolError;

pub(crate) fn build_raw_block(
    header_bytes: &[u8],
    coinbase_tx_bytes: &[u8],
    template_transactions: &[Vec<u8>],
) -> Result<Vec<u8>, PoolError> {
    if header_bytes.len() != 80 {
        return Err(PoolError::Internal(format!(
            "block header length {} != 80",
            header_bytes.len()
        )));
    }

    validate_transaction_bytes("coinbase", coinbase_tx_bytes)?;
    for (index, tx_bytes) in template_transactions.iter().enumerate() {
        validate_transaction_bytes(&format!("template transaction {}", index), tx_bytes)?;
    }

    let tx_count = 1usize
        .checked_add(template_transactions.len())
        .ok_or_else(|| PoolError::Internal("block transaction count overflow".into()))?;

    let mut raw_block = Vec::with_capacity(
        header_bytes.len()
            + coinbase_tx_bytes.len()
            + template_transactions
                .iter()
                .map(|tx| tx.len())
                .sum::<usize>()
            + 9,
    );
    raw_block.extend_from_slice(header_bytes);
    VarInt(tx_count as u64)
        .consensus_encode(&mut raw_block)
        .map_err(|e| PoolError::Internal(format!("block tx count encode failed: {}", e)))?;
    raw_block.extend_from_slice(coinbase_tx_bytes);
    for tx_bytes in template_transactions {
        raw_block.extend_from_slice(tx_bytes);
    }

    Ok(raw_block)
}

fn validate_transaction_bytes(label: &str, tx_bytes: &[u8]) -> Result<(), PoolError> {
    deserialize::<Transaction>(tx_bytes)
        .map(|_| ())
        .map_err(|e| PoolError::Internal(format!("invalid {} bytes: {}", label, e)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::coinbase_builder::{build_coinbase_transaction, CoinbaseBuildInputs};
    use bitcoin::consensus::{deserialize, serialize};
    use bitcoin::Block;

    fn fixture_coinbase_inputs(value: u64) -> CoinbaseBuildInputs {
        CoinbaseBuildInputs {
            height: 100,
            coinbase_value: value,
            payout_script_pubkey: hex::decode("76a91400112233445566778899aabbccddeeff0011223388ac")
                .unwrap(),
            coinbase_aux_flags: None,
            default_witness_commitment: None,
        }
    }

    #[test]
    fn test_build_raw_block_with_coinbase_only_succeeds() {
        let header = [0x11; 80];
        let coinbase_tx = build_coinbase_transaction(&fixture_coinbase_inputs(50)).unwrap();

        let raw_block = build_raw_block(&header, &coinbase_tx, &[]).unwrap();
        let block: Block = deserialize(&raw_block).unwrap();

        assert_eq!(raw_block[..80], header);
        assert_eq!(raw_block[80], 1);
        assert_eq!(block.txdata.len(), 1);
        assert_eq!(serialize(&block.txdata[0]), coinbase_tx);
    }

    #[test]
    fn test_build_raw_block_with_template_transactions_succeeds() {
        let header = [0x22; 80];
        let coinbase_tx = build_coinbase_transaction(&fixture_coinbase_inputs(50)).unwrap();
        let template_tx_1 = build_coinbase_transaction(&fixture_coinbase_inputs(25)).unwrap();
        let template_tx_2 = build_coinbase_transaction(&fixture_coinbase_inputs(10)).unwrap();

        let raw_block = build_raw_block(
            &header,
            &coinbase_tx,
            &[template_tx_1.clone(), template_tx_2.clone()],
        )
        .unwrap();
        let block: Block = deserialize(&raw_block).unwrap();

        assert_eq!(raw_block[80], 3);
        assert_eq!(block.txdata.len(), 3);
        assert_eq!(serialize(&block.txdata[1]), template_tx_1);
        assert_eq!(serialize(&block.txdata[2]), template_tx_2);
    }

    #[test]
    fn test_build_raw_block_preserves_transaction_order_with_coinbase_first() {
        let header = [0x33; 80];
        let coinbase_tx = build_coinbase_transaction(&fixture_coinbase_inputs(50)).unwrap();
        let template_tx_1 = build_coinbase_transaction(&fixture_coinbase_inputs(25)).unwrap();
        let template_tx_2 = build_coinbase_transaction(&fixture_coinbase_inputs(10)).unwrap();

        let raw_block = build_raw_block(
            &header,
            &coinbase_tx,
            &[template_tx_1.clone(), template_tx_2.clone()],
        )
        .unwrap();
        let block: Block = deserialize(&raw_block).unwrap();

        assert_eq!(serialize(&block.txdata[0]), coinbase_tx);
        assert_eq!(serialize(&block.txdata[1]), template_tx_1);
        assert_eq!(serialize(&block.txdata[2]), template_tx_2);
    }

    #[test]
    fn test_build_raw_block_invalid_header_length_fails() {
        let coinbase_tx = build_coinbase_transaction(&fixture_coinbase_inputs(50)).unwrap();

        let err = build_raw_block(&[0u8; 79], &coinbase_tx, &[]).unwrap_err();

        assert!(matches!(err, PoolError::Internal(_)));
        assert!(err.to_string().contains("block header length 79 != 80"));
    }

    #[test]
    fn test_build_raw_block_tx_count_encoding_is_correct_for_small_counts() {
        let header = [0x44; 80];
        let coinbase_tx = build_coinbase_transaction(&fixture_coinbase_inputs(50)).unwrap();
        let template_tx_1 = build_coinbase_transaction(&fixture_coinbase_inputs(25)).unwrap();
        let template_tx_2 = build_coinbase_transaction(&fixture_coinbase_inputs(10)).unwrap();

        let coinbase_only = build_raw_block(&header, &coinbase_tx, &[]).unwrap();
        let with_templates =
            build_raw_block(&header, &coinbase_tx, &[template_tx_1, template_tx_2]).unwrap();

        assert_eq!(coinbase_only[80], 1);
        assert_eq!(with_templates[80], 3);
    }
}
