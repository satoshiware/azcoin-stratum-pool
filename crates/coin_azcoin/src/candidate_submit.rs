#![allow(dead_code)]

use pool_core::{BlockCandidate, BlockSubmitter, Job};

use crate::coinbase_builder::{build_coinbase_transaction, CoinbaseBuildInputs};
use crate::raw_block_builder::build_raw_block;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CandidateSubmissionResult {
    Submitted,
    Rejected(String),
    LocalError(String),
}

pub async fn submit_block_candidate(
    submitter: &dyn BlockSubmitter,
    solved_header_bytes: &[u8],
    job: &Job,
    payout_script_pubkey: Option<&[u8]>,
) -> CandidateSubmissionResult {
    let Some(block_assembly) = job.block_assembly.as_ref() else {
        return CandidateSubmissionResult::LocalError("missing job.block_assembly".to_string());
    };

    let Some(payout_script_pubkey) = payout_script_pubkey.filter(|bytes| !bytes.is_empty()) else {
        return CandidateSubmissionResult::LocalError("missing payout_script_pubkey".to_string());
    };

    let coinbase_tx = match build_coinbase_transaction(&CoinbaseBuildInputs {
        height: block_assembly.height,
        coinbase_value: block_assembly.coinbase_value,
        payout_script_pubkey: payout_script_pubkey.to_vec(),
        coinbase_aux_flags: block_assembly.coinbase_aux_flags.clone(),
        default_witness_commitment: block_assembly.default_witness_commitment.clone(),
    }) {
        Ok(tx) => tx,
        Err(err) => return CandidateSubmissionResult::LocalError(err.to_string()),
    };

    let raw_block = match build_raw_block(
        solved_header_bytes,
        &coinbase_tx,
        &block_assembly.template_transactions,
    ) {
        Ok(block) => block,
        Err(err) => return CandidateSubmissionResult::LocalError(err.to_string()),
    };

    match submitter
        .submit_block(BlockCandidate {
            block_hash: [0u8; 32],
            height: block_assembly.height,
            raw_block,
        })
        .await
    {
        Ok(true) => CandidateSubmissionResult::Submitted,
        Ok(false) => {
            CandidateSubmissionResult::Rejected("block submitter returned false".to_string())
        }
        Err(message) => {
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
    use pool_core::{BlockAssemblyData, Job};
    use std::sync::{Arc, Mutex};

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

    fn fixture_job() -> Job {
        let mut job = Job::placeholder();
        job.block_assembly = Some(BlockAssemblyData {
            height: 100,
            coinbase_value: 5_000_000_000,
            coinbase_aux_flags: Some(vec![0xde, 0xad, 0xbe, 0xef]),
            template_transactions: vec![build_coinbase_transaction(&CoinbaseBuildInputs {
                height: 101,
                coinbase_value: 25,
                payout_script_pubkey: hex::decode(
                    "76a91400112233445566778899aabbccddeeff0011223388ac",
                )
                .unwrap(),
                coinbase_aux_flags: None,
                default_witness_commitment: None,
            })
            .unwrap()],
            default_witness_commitment: Some(
                hex::decode("6a24aa21a9ed11223344556677889900aabbccddeeff00112233445566778899")
                    .unwrap(),
            ),
        });
        job
    }

    fn fixture_payout_script() -> Vec<u8> {
        hex::decode("76a91400112233445566778899aabbccddeeff0011223388ac").unwrap()
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
            Some(&fixture_payout_script()),
        ));

        assert_eq!(
            result,
            CandidateSubmissionResult::LocalError("missing job.block_assembly".to_string())
        );
    }

    #[test]
    fn test_submit_block_candidate_missing_payout_script_fails_clearly() {
        let submitter = RecordingSubmitter {
            submitted: Arc::new(Mutex::new(Vec::new())),
            response: TestSubmitResponse::Ok,
        };
        let job = fixture_job();

        let result =
            build_runtime().block_on(submit_block_candidate(&submitter, &[0x11; 80], &job, None));

        assert_eq!(
            result,
            CandidateSubmissionResult::LocalError("missing payout_script_pubkey".to_string())
        );
    }

    #[test]
    fn test_submit_block_candidate_happy_path_calls_submitter_with_assembled_raw_block() {
        let submitted = Arc::new(Mutex::new(Vec::new()));
        let submitter = RecordingSubmitter {
            submitted: submitted.clone(),
            response: TestSubmitResponse::Ok,
        };
        let job = fixture_job();
        let payout_script = fixture_payout_script();
        let header = [0x11; 80];

        let result = build_runtime().block_on(submit_block_candidate(
            &submitter,
            &header,
            &job,
            Some(&payout_script),
        ));

        assert_eq!(result, CandidateSubmissionResult::Submitted);

        let block_assembly = job.block_assembly.as_ref().unwrap();
        let expected_coinbase = build_coinbase_transaction(&CoinbaseBuildInputs {
            height: block_assembly.height,
            coinbase_value: block_assembly.coinbase_value,
            payout_script_pubkey: payout_script.clone(),
            coinbase_aux_flags: block_assembly.coinbase_aux_flags.clone(),
            default_witness_commitment: block_assembly.default_witness_commitment.clone(),
        })
        .unwrap();
        let expected_raw_block = build_raw_block(
            &header,
            &expected_coinbase,
            &block_assembly.template_transactions,
        )
        .unwrap();

        assert_eq!(submitted.lock().unwrap().as_slice(), &[expected_raw_block]);
    }

    #[test]
    fn test_submit_block_candidate_daemon_reject_reason_is_propagated_clearly() {
        let submitter = RecordingSubmitter {
            submitted: Arc::new(Mutex::new(Vec::new())),
            response: TestSubmitResponse::DaemonError(
                "submitblock rejected: high-hash".to_string(),
            ),
        };
        let job = fixture_job();
        let payout_script = fixture_payout_script();

        let result = build_runtime().block_on(submit_block_candidate(
            &submitter,
            &[0x11; 80],
            &job,
            Some(&payout_script),
        ));

        assert_eq!(
            result,
            CandidateSubmissionResult::Rejected("high-hash".to_string())
        );
    }

    #[test]
    fn test_submit_block_candidate_uses_coinbase_builder_and_raw_block_builder_path() {
        let submitted = Arc::new(Mutex::new(Vec::new()));
        let submitter = RecordingSubmitter {
            submitted: submitted.clone(),
            response: TestSubmitResponse::Ok,
        };
        let job = fixture_job();
        let payout_script = fixture_payout_script();
        let header = [0x22; 80];

        let _ = build_runtime().block_on(submit_block_candidate(
            &submitter,
            &header,
            &job,
            Some(&payout_script),
        ));

        let block_assembly = job.block_assembly.as_ref().unwrap();
        let expected_raw_block = build_raw_block(
            &header,
            &build_coinbase_transaction(&CoinbaseBuildInputs {
                height: block_assembly.height,
                coinbase_value: block_assembly.coinbase_value,
                payout_script_pubkey: payout_script,
                coinbase_aux_flags: block_assembly.coinbase_aux_flags.clone(),
                default_witness_commitment: block_assembly.default_witness_commitment.clone(),
            })
            .unwrap(),
            &block_assembly.template_transactions,
        )
        .unwrap();

        assert_eq!(submitted.lock().unwrap()[0], expected_raw_block);
    }
}
