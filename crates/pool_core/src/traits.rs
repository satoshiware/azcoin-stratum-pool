//! Domain service interfaces. Implementations live in protocol adapters, coin, storage.

use crate::{
    BlockCandidate, Job, PayoutRecord, Round, ShareResult, ShareSubmission, WorkerIdentity,
};
use async_trait::async_trait;

/// Provides jobs to miners. Implemented by coin_azcoin (block template) or stubs.
#[async_trait]
pub trait JobSource: Send + Sync {
    async fn current_job(&self) -> Option<Job>;
}

/// Processes share submissions. Implemented by pool_core or a dedicated service.
#[async_trait]
pub trait ShareProcessor: Send + Sync {
    async fn process_share(&self, share: ShareSubmission) -> ShareResult;
}

/// Manages mining rounds. Implemented by pool_core or storage-backed service.
#[async_trait]
pub trait RoundManager: Send + Sync {
    async fn current_round(&self) -> Option<Round>;
    async fn close_round(&self, round_id: &str) -> Result<(), String>;
}

/// Submits blocks to the chain. Implemented by coin_azcoin.
#[async_trait]
pub trait BlockSubmitter: Send + Sync {
    async fn submit_block(&self, block: BlockCandidate) -> Result<bool, String>;
}

/// Balance ledger for workers. Implemented by storage or in-memory stub.
#[async_trait]
pub trait BalanceLedger: Send + Sync {
    async fn get_balance(&self, worker: &WorkerIdentity) -> i64;
    async fn credit(&self, worker: &WorkerIdentity, amount: i64) -> Result<(), String>;
}

/// Payout execution. Placeholder—not implemented yet.
#[async_trait]
pub trait PayoutExecutor: Send + Sync {
    async fn execute_payout(
        &self,
        worker: &WorkerIdentity,
        amount: i64,
    ) -> Result<PayoutRecord, String>;
}

/// Worker persistence. Implemented by storage.
#[async_trait]
pub trait WorkerRepository: Send + Sync {
    async fn get_worker(&self, id: &str) -> Option<WorkerIdentity>;
    async fn upsert_worker(&self, worker: &WorkerIdentity) -> Result<(), String>;
}

/// Share persistence. Implemented by storage.
#[async_trait]
pub trait ShareRepository: Send + Sync {
    async fn store_share(
        &self,
        share: &ShareSubmission,
        result: &ShareResult,
    ) -> Result<(), String>;
}

/// Round persistence. Implemented by storage.
#[async_trait]
pub trait RoundRepository: Send + Sync {
    async fn get_round(&self, round_id: &str) -> Option<Round>;
    async fn insert_round(&self, round: &Round) -> Result<(), String>;
}
