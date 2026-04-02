#![allow(dead_code)]

use bitcoin::absolute::LockTime;
use bitcoin::consensus::encode::Encodable;
use bitcoin::consensus::serialize;
use bitcoin::transaction::Version;
use bitcoin::{Amount, OutPoint, ScriptBuf, Sequence, Transaction, TxIn, TxOut, Witness};
use common::PoolError;

pub(crate) struct CoinbaseBuildInputs {
    pub height: u64,
    pub coinbase_value: u64,
    pub payout_script_pubkey: Vec<u8>,
    pub coinbase_aux_flags: Option<Vec<u8>>,
    pub default_witness_commitment: Option<Vec<u8>>,
}

pub(crate) fn build_coinbase_transaction(
    inputs: &CoinbaseBuildInputs,
) -> Result<Vec<u8>, PoolError> {
    if inputs.payout_script_pubkey.is_empty() {
        return Err(PoolError::Config(
            "coinbase payout script_pubkey cannot be empty".into(),
        ));
    }

    let script_sig = build_coinbase_script_sig(inputs.height, inputs.coinbase_aux_flags.as_deref());

    let mut output = vec![TxOut {
        value: Amount::from_sat(inputs.coinbase_value),
        script_pubkey: ScriptBuf::from_bytes(inputs.payout_script_pubkey.clone()),
    }];

    let witness = if let Some(commitment_script) = inputs.default_witness_commitment.as_ref() {
        output.push(TxOut {
            value: Amount::from_sat(0),
            script_pubkey: ScriptBuf::from_bytes(commitment_script.clone()),
        });
        Witness::from(vec![vec![0u8; 32]])
    } else {
        Witness::default()
    };

    let tx = Transaction {
        version: Version::ONE,
        lock_time: LockTime::ZERO,
        input: vec![TxIn {
            previous_output: OutPoint::null(),
            script_sig: ScriptBuf::from_bytes(script_sig),
            sequence: Sequence::MAX,
            witness,
        }],
        output,
    };

    Ok(serialize(&tx))
}

/// Serialize a Transaction in legacy (non-witness) format:
/// version || inputs || outputs || locktime.
/// double_sha256 of these bytes yields the txid used in the header merkle tree.
pub(crate) fn serialize_no_witness(tx: &Transaction) -> Vec<u8> {
    let mut buf = Vec::new();
    tx.version.consensus_encode(&mut buf).expect("version");
    tx.input.consensus_encode(&mut buf).expect("inputs");
    tx.output.consensus_encode(&mut buf).expect("outputs");
    tx.lock_time.consensus_encode(&mut buf).expect("locktime");
    buf
}

fn build_coinbase_script_sig(height: u64, coinbase_aux_flags: Option<&[u8]>) -> Vec<u8> {
    let mut script_sig = Vec::new();
    push_data(&mut script_sig, &encode_script_num(height));

    if let Some(flags) = coinbase_aux_flags {
        if !flags.is_empty() {
            push_data(&mut script_sig, flags);
        }
    }

    script_sig
}

fn encode_script_num(value: u64) -> Vec<u8> {
    if value == 0 {
        return Vec::new();
    }

    let mut encoded = Vec::new();
    let mut remaining = value;
    while remaining > 0 {
        encoded.push((remaining & 0xff) as u8);
        remaining >>= 8;
    }

    if encoded.last().is_some_and(|byte| byte & 0x80 != 0) {
        encoded.push(0);
    }

    encoded
}

fn push_data(buf: &mut Vec<u8>, data: &[u8]) {
    let len = data.len();
    match len {
        0..=0x4b => buf.push(len as u8),
        0x4c..=0xff => {
            buf.push(0x4c);
            buf.push(len as u8);
        }
        0x100..=0xffff => {
            buf.push(0x4d);
            buf.extend_from_slice(&(len as u16).to_le_bytes());
        }
        _ => {
            buf.push(0x4e);
            buf.extend_from_slice(&(len as u32).to_le_bytes());
        }
    }
    buf.extend_from_slice(data);
}

#[cfg(test)]
mod tests {
    use super::*;
    use bitcoin::consensus::deserialize;

    fn fixture_inputs() -> CoinbaseBuildInputs {
        CoinbaseBuildInputs {
            height: 100,
            coinbase_value: 5_000_000_000,
            payout_script_pubkey: hex::decode("76a91400112233445566778899aabbccddeeff0011223388ac")
                .unwrap(),
            coinbase_aux_flags: None,
            default_witness_commitment: None,
        }
    }

    #[test]
    fn test_build_coinbase_transaction_non_segwit_succeeds() {
        let tx_bytes = build_coinbase_transaction(&fixture_inputs()).unwrap();
        let tx: Transaction = deserialize(&tx_bytes).unwrap();

        assert_eq!(tx.input.len(), 1);
        assert_eq!(tx.output.len(), 1);
        assert!(tx.input[0].witness.is_empty());
        assert_eq!(tx.output[0].value.to_sat(), 5_000_000_000);
    }

    #[test]
    fn test_build_coinbase_transaction_with_witness_commitment_succeeds() {
        let mut inputs = fixture_inputs();
        inputs.default_witness_commitment = Some(
            hex::decode("6a24aa21a9ed11223344556677889900aabbccddeeff00112233445566778899")
                .unwrap(),
        );

        let tx_bytes = build_coinbase_transaction(&inputs).unwrap();
        let tx: Transaction = deserialize(&tx_bytes).unwrap();

        assert_eq!(tx.output.len(), 2);
        assert_eq!(
            tx.output[1].script_pubkey.as_bytes(),
            inputs
                .default_witness_commitment
                .as_ref()
                .unwrap()
                .as_slice()
        );
        assert_eq!(tx.output[1].value.to_sat(), 0);
        assert_eq!(tx.input[0].witness.len(), 1);
        assert_eq!(tx.input[0].witness.iter().next().unwrap(), [0u8; 32]);
    }

    #[test]
    fn test_build_coinbase_transaction_includes_payout_script() {
        let inputs = fixture_inputs();
        let tx_bytes = build_coinbase_transaction(&inputs).unwrap();
        let tx: Transaction = deserialize(&tx_bytes).unwrap();

        assert_eq!(
            tx.output[0].script_pubkey.as_bytes(),
            inputs.payout_script_pubkey
        );
    }

    #[test]
    fn test_build_coinbase_transaction_coinbase_aux_flags_affect_script_sig() {
        let mut without_flags = fixture_inputs();
        let mut with_flags = fixture_inputs();
        with_flags.coinbase_aux_flags = Some(vec![0xde, 0xad, 0xbe, 0xef]);

        let tx_without_flags = build_coinbase_transaction(&without_flags).unwrap();
        let tx_with_flags = build_coinbase_transaction(&with_flags).unwrap();
        let tx: Transaction = deserialize(&tx_with_flags).unwrap();

        assert_ne!(tx_without_flags, tx_with_flags);
        assert!(tx.input[0]
            .script_sig
            .as_bytes()
            .windows(4)
            .any(|window| window == [0xde, 0xad, 0xbe, 0xef]));
        without_flags.coinbase_aux_flags = Some(vec![0xde, 0xad, 0xbe, 0xef]);
        assert_eq!(tx.input[0].script_sig.as_bytes()[0], 1);
        assert_eq!(tx.input[0].script_sig.as_bytes()[1], 100);
    }

    #[test]
    fn test_build_coinbase_transaction_empty_payout_script_fails() {
        let mut inputs = fixture_inputs();
        inputs.payout_script_pubkey.clear();

        let err = build_coinbase_transaction(&inputs).unwrap_err();
        assert!(matches!(err, PoolError::Config(_)));
        assert!(err
            .to_string()
            .contains("coinbase payout script_pubkey cannot be empty"));
    }

    #[test]
    fn test_serialize_no_witness_excludes_marker_flag_and_witness_data() {
        let mut inputs = fixture_inputs();
        inputs.default_witness_commitment = Some(
            hex::decode("6a24aa21a9ed11223344556677889900aabbccddeeff00112233445566778899")
                .unwrap(),
        );

        let witness_bytes = build_coinbase_transaction(&inputs).unwrap();
        let tx: Transaction = deserialize(&witness_bytes).unwrap();
        let no_witness_bytes = serialize_no_witness(&tx);

        assert!(
            witness_bytes.len() > no_witness_bytes.len(),
            "witness serialization must be longer"
        );
        // Non-witness: byte 4 is input count (0x01), not segwit marker (0x00)
        assert_eq!(no_witness_bytes[4], 0x01);
        // Witness: byte 4 is segwit marker (0x00)
        assert_eq!(witness_bytes[4], 0x00);

        // Both deserialize to a transaction with the same outputs
        let tx_roundtrip: Transaction = deserialize(&no_witness_bytes).unwrap();
        assert_eq!(tx.output.len(), tx_roundtrip.output.len());
        assert_eq!(tx.input[0].script_sig, tx_roundtrip.input[0].script_sig);
        // Non-witness deserialization has empty witness
        assert!(tx_roundtrip.input[0].witness.is_empty());
    }
}
