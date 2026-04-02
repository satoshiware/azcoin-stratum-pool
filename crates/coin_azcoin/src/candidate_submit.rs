#![allow(dead_code)]

use pool_core::{BlockCandidate, BlockSubmitter, Job};
use tracing::{info, warn};

use crate::raw_block_builder::build_raw_block;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CandidateSubmissionResult {
    Submitted,
    Rejected(String),
    LocalError(String),
}

/// Reconstruct the exact coinbase the miner solved:
/// job.coinbase_part1 + extranonce1 + extra_nonce2 + job.coinbase_part2
fn reconstruct_miner_coinbase(job: &Job, extranonce1: &[u8], extra_nonce2: &[u8]) -> Vec<u8> {
    let mut coinbase = Vec::with_capacity(
        job.coinbase_part1.len()
            + extranonce1.len()
            + extra_nonce2.len()
            + job.coinbase_part2.len(),
    );
    coinbase.extend_from_slice(&job.coinbase_part1);
    coinbase.extend_from_slice(extranonce1);
    coinbase.extend_from_slice(extra_nonce2);
    coinbase.extend_from_slice(&job.coinbase_part2);
    coinbase
}

/// Deserialize a non-witness coinbase and add the segwit witness nonce
/// so the raw block contains the witness-serialized coinbase the daemon expects.
fn restore_coinbase_witness(coinbase_no_witness: &[u8]) -> Result<Vec<u8>, String> {
    let mut tx: bitcoin::Transaction =
        bitcoin::consensus::deserialize(coinbase_no_witness)
            .map_err(|e| format!("coinbase deserialize for witness: {}", e))?;
    tx.input[0].witness = bitcoin::Witness::from(vec![vec![0u8; 32]]);
    Ok(bitcoin::consensus::serialize(&tx))
}

pub async fn submit_block_candidate(
    submitter: &dyn BlockSubmitter,
    solved_header_bytes: &[u8],
    job: &Job,
    extranonce1: &[u8],
    extra_nonce2: &[u8],
) -> CandidateSubmissionResult {
    let Some(block_assembly) = job.block_assembly.as_ref() else {
        return CandidateSubmissionResult::LocalError("missing job.block_assembly".to_string());
    };

    let coinbase_no_witness = reconstruct_miner_coinbase(job, extranonce1, extra_nonce2);

    // If the block uses segwit (has witness commitment), the raw block must
    // contain the witness-serialized coinbase with the 32-byte witness nonce.
    let coinbase_tx = if block_assembly.default_witness_commitment.is_some() {
        match restore_coinbase_witness(&coinbase_no_witness) {
            Ok(bytes) => bytes,
            Err(e) => return CandidateSubmissionResult::LocalError(e),
        }
    } else {
        coinbase_no_witness
    };

    let raw_block = match build_raw_block(
        solved_header_bytes,
        &coinbase_tx,
        &block_assembly.template_transactions,
    ) {
        Ok(block) => block,
        Err(err) => return CandidateSubmissionResult::LocalError(err.to_string()),
    };

    let candidate = BlockCandidate {
        block_hash: [0u8; 32],
        height: block_assembly.height,
        raw_block,
    };

    info!(
        height = block_assembly.height,
        raw_block_len = candidate.raw_block.len(),
        solved_header_len = solved_header_bytes.len(),
        "submitblock attempt"
    );

    match submitter.submit_block(candidate).await {
        Ok(true) => {
            info!(height = block_assembly.height, "submitblock accepted");
            CandidateSubmissionResult::Submitted
        }
        Ok(false) => {
            warn!(height = block_assembly.height, "submitblock returned false");
            CandidateSubmissionResult::Rejected("block submitter returned false".to_string())
        }
        Err(message) => {
            warn!(
                height = block_assembly.height,
                error = %message,
                "submitblock errored"
            );
            if let Some(reason) = message.strip_prefix("submitblock rejected: ") {
                CandidateSubmissionResult::Rejected(reason.to_string())
            } else {
                CandidateSubmissionResult::LocalError(message)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::coinbase_builder::{
        build_coinbase_transaction, serialize_no_witness, CoinbaseBuildInputs,
    };
    use pool_core::{BlockAssemblyData, Job};
    use std::sync::{Arc, Mutex};

    const EXTRANONCE_PLACEHOLDER: [u8; 8] = [0xfa, 0xce, 0xb0, 0x0c, 0xde, 0xad, 0xbe, 0xef];

    #[derive(Clone)]
    struct RecordingSubmitter {
        submitted: Arc<Mutex<Vec<Vec<u8>>>>,
        response: TestSubmitResponse,
    }

    #[derive(Clone)]
    enum TestSubmitResponse {
        Ok,
        DaemonError(String),
        InternalError(String),
    }

    #[async_trait::async_trait]
    impl BlockSubmitter for RecordingSubmitter {
        async fn submit_block(&self, block: BlockCandidate) -> Result<bool, String> {
            self.submitted.lock().unwrap().push(block.raw_block);
            match &self.response {
                TestSubmitResponse::Ok => Ok(true),
                TestSubmitResponse::DaemonError(message) => Err(message.clone()),
                TestSubmitResponse::InternalError(message) => Err(message.clone()),
            }
        }
    }

    fn fixture_payout_script() -> Vec<u8> {
        hex::decode("76a91400112233445566778899aabbccddeeff0011223388ac").unwrap()
    }

    /// Build a job with valid coinbase_part1/part2 that, when combined with
    /// extranonce1 + extranonce2, produce a valid Bitcoin transaction.
    fn fixture_job_with_valid_coinbase_parts() -> Job {
        let mut flags_with_placeholder = vec![0xde, 0xad, 0xbe, 0xef];
        flags_with_placeholder.extend_from_slice(&EXTRANONCE_PLACEHOLDER);

        let coinbase_tx = build_coinbase_transaction(&CoinbaseBuildInputs {
            height: 100,
            coinbase_value: 5_000_000_000,
            payout_script_pubkey: fixture_payout_script(),
            coinbase_aux_flags: Some(flags_with_placeholder),
            default_witness_commitment: None,
        })
        .unwrap();

        let split_at = coinbase_tx
            .windows(EXTRANONCE_PLACEHOLDER.len())
            .position(|w| w == EXTRANONCE_PLACEHOLDER)
            .expect("placeholder must appear in coinbase");

        let coinbase_part1 = coinbase_tx[..split_at].to_vec();
        let coinbase_part2 = coinbase_tx[split_at + EXTRANONCE_PLACEHOLDER.len()..].to_vec();

        Job {
            job_id: "100".to_string(),
            prev_hash: [0u8; 32],
            coinbase_part1,
            coinbase_part2,
            merkle_branch: vec![],
            version: 0x20000000,
            nbits: 0x1d00ffff,
            ntime: 0,
            clean_jobs: true,
            block_assembly: Some(BlockAssemblyData {
                height: 100,
                coinbase_value: 5_000_000_000,
                coinbase_aux_flags: Some(vec![0xde, 0xad, 0xbe, 0xef]),
                template_transactions: vec![],
                default_witness_commitment: None,
            }),
        }
    }

    fn build_runtime() -> tokio::runtime::Runtime {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
    }

    #[test]
    fn test_submit_block_candidate_missing_block_assembly_fails_clearly() {
        let submitter = RecordingSubmitter {
            submitted: Arc::new(Mutex::new(Vec::new())),
            response: TestSubmitResponse::Ok,
        };
        let job = Job::placeholder();

        let result = build_runtime().block_on(submit_block_candidate(
            &submitter,
            &[0x11; 80],
            &job,
            &[0x00; 4],
            &[0x00; 4],
        ));

        assert_eq!(
            result,
            CandidateSubmissionResult::LocalError("missing job.block_assembly".to_string())
        );
    }

    #[test]
    fn test_reconstruct_miner_coinbase_is_part1_en1_en2_part2() {
        let job = fixture_job_with_valid_coinbase_parts();
        let extranonce1 = [0xaa, 0xbb, 0xcc, 0xdd];
        let extra_nonce2 = [0x11, 0x22, 0x33, 0x44];

        let coinbase = reconstruct_miner_coinbase(&job, &extranonce1, &extra_nonce2);

        let mut expected = Vec::new();
        expected.extend_from_slice(&job.coinbase_part1);
        expected.extend_from_slice(&extranonce1);
        expected.extend_from_slice(&extra_nonce2);
        expected.extend_from_slice(&job.coinbase_part2);
        assert_eq!(coinbase, expected);
    }

    #[test]
    fn test_submit_block_candidate_uses_miner_solved_coinbase() {
        let submitted = Arc::new(Mutex::new(Vec::new()));
        let submitter = RecordingSubmitter {
            submitted: submitted.clone(),
            response: TestSubmitResponse::Ok,
        };
        let job = fixture_job_with_valid_coinbase_parts();
        let header = [0x11; 80];
        let extranonce1 = [0xaa, 0xbb, 0xcc, 0xdd];
        let extra_nonce2 = [0x11, 0x22, 0x33, 0x44];

        let result = build_runtime().block_on(submit_block_candidate(
            &submitter,
            &header,
            &job,
            &extranonce1,
            &extra_nonce2,
        ));

        assert_eq!(result, CandidateSubmissionResult::Submitted);

        let expected_coinbase = reconstruct_miner_coinbase(&job, &extranonce1, &extra_nonce2);
        let expected_raw_block =
            build_raw_block(&header, &expected_coinbase, &job.block_assembly.as_ref().unwrap().template_transactions)
                .unwrap();

        assert_eq!(submitted.lock().unwrap().as_slice(), &[expected_raw_block]);
    }

    #[test]
    fn test_submit_block_candidate_raw_block_contains_miner_coinbase_bytes() {
        let submitted = Arc::new(Mutex::new(Vec::new()));
        let submitter = RecordingSubmitter {
            submitted: submitted.clone(),
            response: TestSubmitResponse::Ok,
        };
        let job = fixture_job_with_valid_coinbase_parts();
        let header = [0x22; 80];
        let extranonce1 = [0xaa, 0xbb, 0xcc, 0xdd];
        let extra_nonce2 = [0x11, 0x22, 0x33, 0x44];

        let result = build_runtime().block_on(submit_block_candidate(
            &submitter, &header, &job, &extranonce1, &extra_nonce2,
        ));
        assert_eq!(result, CandidateSubmissionResult::Submitted);

        let raw_block = submitted.lock().unwrap()[0].clone();
        assert_eq!(&raw_block[..80], &header);

        let expected_coinbase = reconstruct_miner_coinbase(&job, &extranonce1, &extra_nonce2);
        assert!(
            raw_block[80..]
                .windows(expected_coinbase.len())
                .any(|w| w == expected_coinbase.as_slice()),
            "raw block must contain the exact miner-solved coinbase bytes"
        );
    }

    #[test]
    fn test_submit_block_candidate_daemon_reject_reason_is_propagated_clearly() {
        let submitter = RecordingSubmitter {
            submitted: Arc::new(Mutex::new(Vec::new())),
            response: TestSubmitResponse::DaemonError(
                "submitblock rejected: high-hash".to_string(),
            ),
        };
        let job = fixture_job_with_valid_coinbase_parts();

        let result = build_runtime().block_on(submit_block_candidate(
            &submitter,
            &[0x11; 80],
            &job,
            &[0x00; 4],
            &[0x00; 4],
        ));

        assert_eq!(
            result,
            CandidateSubmissionResult::Rejected("high-hash".to_string())
        );
    }

    #[test]
    fn test_submit_block_candidate_internal_error_is_local_error() {
        let submitter = RecordingSubmitter {
            submitted: Arc::new(Mutex::new(Vec::new())),
            response: TestSubmitResponse::InternalError("connection refused".to_string()),
        };
        let job = fixture_job_with_valid_coinbase_parts();

        let result = build_runtime().block_on(submit_block_candidate(
            &submitter,
            &[0x11; 80],
            &job,
            &[0x00; 4],
            &[0x00; 4],
        ));

        assert_eq!(
            result,
            CandidateSubmissionResult::LocalError("connection refused".to_string())
        );
    }

    fn fixture_witness_commitment() -> Vec<u8> {
        hex::decode("6a24aa21a9ed11223344556677889900aabbccddeeff00112233445566778899")
            .unwrap()
    }

    /// Build a segwit job: coinbase_part1/part2 are NON-WITNESS bytes (matching
    /// what template_mapper now produces), but block_assembly has a witness commitment.
    fn fixture_job_with_segwit_coinbase_parts() -> Job {
        let witness_commitment = fixture_witness_commitment();
        let mut flags_with_placeholder = vec![0xde, 0xad, 0xbe, 0xef];
        flags_with_placeholder.extend_from_slice(&EXTRANONCE_PLACEHOLDER);

        let coinbase_tx_bytes = build_coinbase_transaction(&CoinbaseBuildInputs {
            height: 200,
            coinbase_value: 5_000_000_000,
            payout_script_pubkey: fixture_payout_script(),
            coinbase_aux_flags: Some(flags_with_placeholder),
            default_witness_commitment: Some(witness_commitment.clone()),
        })
        .unwrap();

        let coinbase_tx: bitcoin::Transaction =
            bitcoin::consensus::deserialize(&coinbase_tx_bytes).unwrap();
        let coinbase_no_witness = serialize_no_witness(&coinbase_tx);

        let split_at = coinbase_no_witness
            .windows(EXTRANONCE_PLACEHOLDER.len())
            .position(|w| w == EXTRANONCE_PLACEHOLDER)
            .expect("placeholder must appear in non-witness coinbase");

        let coinbase_part1 = coinbase_no_witness[..split_at].to_vec();
        let coinbase_part2 =
            coinbase_no_witness[split_at + EXTRANONCE_PLACEHOLDER.len()..].to_vec();

        Job {
            job_id: "200".to_string(),
            prev_hash: [0u8; 32],
            coinbase_part1,
            coinbase_part2,
            merkle_branch: vec![],
            version: 0x20000000,
            nbits: 0x1d00ffff,
            ntime: 0,
            clean_jobs: true,
            block_assembly: Some(BlockAssemblyData {
                height: 200,
                coinbase_value: 5_000_000_000,
                coinbase_aux_flags: Some(vec![0xde, 0xad, 0xbe, 0xef]),
                template_transactions: vec![],
                default_witness_commitment: Some(witness_commitment),
            }),
        }
    }

    #[test]
    fn test_submit_segwit_block_restores_witness_in_raw_block() {
        let submitted = Arc::new(Mutex::new(Vec::new()));
        let submitter = RecordingSubmitter {
            submitted: submitted.clone(),
            response: TestSubmitResponse::Ok,
        };
        let job = fixture_job_with_segwit_coinbase_parts();
        let header = [0x33; 80];
        let extranonce1 = [0xaa, 0xbb, 0xcc, 0xdd];
        let extra_nonce2 = [0x11, 0x22, 0x33, 0x44];

        let result = build_runtime().block_on(submit_block_candidate(
            &submitter, &header, &job, &extranonce1, &extra_nonce2,
        ));
        assert_eq!(result, CandidateSubmissionResult::Submitted);

        let raw_block = submitted.lock().unwrap()[0].clone();

        // Reconstructed non-witness coinbase (what miners hash for txid)
        let coinbase_no_witness = reconstruct_miner_coinbase(&job, &extranonce1, &extra_nonce2);
        // The raw block must NOT contain the non-witness bytes verbatim,
        // because witness restoration adds marker/flag/witness data.
        let witness_coinbase = restore_coinbase_witness(&coinbase_no_witness).unwrap();
        assert!(witness_coinbase.len() > coinbase_no_witness.len());

        // The raw block should contain the witness-serialized coinbase
        assert!(
            raw_block[81..]
                .windows(witness_coinbase.len())
                .any(|w| w == witness_coinbase.as_slice()),
            "raw block must contain the witness-serialized coinbase"
        );

        // Verify the witness coinbase has segwit marker at byte 4
        assert_eq!(witness_coinbase[4], 0x00, "segwit marker");
        assert_eq!(witness_coinbase[5], 0x01, "segwit flag");

        // Verify the deserialized witness coinbase has the witness nonce
        let tx: bitcoin::Transaction =
            bitcoin::consensus::deserialize(&witness_coinbase).unwrap();
        assert_eq!(tx.input[0].witness.len(), 1);
        assert_eq!(
            tx.input[0].witness.iter().next().unwrap(),
            [0u8; 32]
        );
    }

    #[test]
    fn test_restore_coinbase_witness_adds_32_byte_nonce() {
        let job = fixture_job_with_segwit_coinbase_parts();
        let coinbase_no_witness = reconstruct_miner_coinbase(&job, &[0; 4], &[0; 4]);

        let witness_bytes = restore_coinbase_witness(&coinbase_no_witness).unwrap();
        let tx: bitcoin::Transaction =
            bitcoin::consensus::deserialize(&witness_bytes).unwrap();

        assert_eq!(tx.input[0].witness.len(), 1);
        assert_eq!(tx.input[0].witness.iter().next().unwrap(), [0u8; 32]);
        assert!(witness_bytes.len() > coinbase_no_witness.len());
    }
}
