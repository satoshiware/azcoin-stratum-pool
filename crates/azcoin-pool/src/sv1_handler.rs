use coin_azcoin::{build_solved_block_header, submit_block_candidate, CandidateSubmissionResult};
use pool_core::{
    BlockSubmitter, JobSource, ShareProcessor, ShareResult, ShareSubmission, WorkerIdentity,
};
use std::net::SocketAddr;
use std::sync::Arc;
use tracing::{info, warn};

/// Session handler: stats, workers, job source, share processor, block submission.
pub struct Sv1SessionHandler {
    pub stats: Arc<pool_core::InMemoryStatsSnapshot>,
    pub worker_registry: Arc<pool_core::InMemoryWorkerRegistry>,
    pub job_source: Arc<dyn JobSource>,
    pub job_registry: Arc<pool_core::ActiveJobRegistry>,
    pub share_processor: Arc<dyn ShareProcessor>,
    pub block_submitter: Arc<dyn BlockSubmitter>,
    pub payout_script_pubkey: Option<Vec<u8>>,
}

impl Sv1SessionHandler {
    async fn maybe_submit_block_candidate(&self, share: &ShareSubmission) {
        let Some(job) = self.job_registry.get_job(&share.job_id).await else {
            warn!(job_id = %share.job_id, "block share found but job missing from registry");
            return;
        };

        let extranonce1 = match &share.validation_context {
            Some(ctx) => match &ctx.extranonce1_hex {
                Some(hex) => match hex::decode(hex) {
                    Ok(bytes) => bytes,
                    Err(_) => {
                        warn!(
                            job_id = %share.job_id,
                            worker = %share.worker.id,
                            "block share found but extranonce1 was invalid hex"
                        );
                        return;
                    }
                },
                None => vec![0u8; 4],
            },
            None => vec![0u8; 4],
        };

        let solved_header = build_solved_block_header(&job, share, &extranonce1);
        match submit_block_candidate(
            self.block_submitter.as_ref(),
            &solved_header,
            &job,
            self.payout_script_pubkey.as_deref(),
        )
        .await
        {
            CandidateSubmissionResult::Submitted => {
                info!(job_id = %share.job_id, worker = %share.worker.id, "submitted block candidate");
            }
            CandidateSubmissionResult::Rejected(reason) => {
                warn!(
                    job_id = %share.job_id,
                    worker = %share.worker.id,
                    reason = %reason,
                    "block candidate rejected by daemon"
                );
            }
            CandidateSubmissionResult::LocalError(message) => {
                warn!(
                    job_id = %share.job_id,
                    worker = %share.worker.id,
                    error = %message,
                    "block candidate submission failed locally"
                );
            }
        }
    }
}

#[async_trait::async_trait]
impl protocol_sv1::SessionEventHandler for Sv1SessionHandler {
    fn on_connect(&self, _peer: SocketAddr) {
        self.stats.record_connection();
    }

    fn on_disconnect(&self, _peer: SocketAddr) {
        self.stats.record_disconnection();
    }

    async fn on_authorize(&self, username: &str) -> Result<Option<pool_core::Job>, String> {
        let worker = WorkerIdentity::new(username);
        self.worker_registry.register(worker).await;
        Ok(self.job_source.current_job().await)
    }

    async fn on_notify_sent(&self, job: pool_core::Job) {
        self.job_registry.register(job).await;
    }

    async fn on_submit(&self, share: pool_core::ShareSubmission) -> pool_core::ShareResult {
        let result = self.share_processor.process_share(share.clone()).await;
        if matches!(result, ShareResult::Block) {
            self.maybe_submit_block_candidate(&share).await;
        }
        result
    }
}
