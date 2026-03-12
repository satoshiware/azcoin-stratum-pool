//! Repository implementations. Stub implementations for bootstrap.

use async_trait::async_trait;
use pool_core::{
    Round, RoundRepository, ShareRepository, ShareResult, ShareSubmission, WorkerIdentity,
    WorkerRepository,
};

/// In-memory worker repository. Replace with PostgreSQL when database feature is enabled.
pub struct StubWorkerRepository;

#[async_trait]
impl WorkerRepository for StubWorkerRepository {
    async fn get_worker(&self, _id: &str) -> Option<WorkerIdentity> {
        None
    }

    async fn upsert_worker(&self, worker: &WorkerIdentity) -> Result<(), String> {
        let _ = worker;
        Ok(())
    }
}

/// In-memory share repository. Replace with PostgreSQL when database feature is enabled.
pub struct StubShareRepository;

#[async_trait]
impl ShareRepository for StubShareRepository {
    async fn store_share(
        &self,
        _share: &ShareSubmission,
        _result: &ShareResult,
    ) -> Result<(), String> {
        Ok(())
    }
}

/// In-memory round repository. Replace with PostgreSQL when database feature is enabled.
pub struct StubRoundRepository;

#[async_trait]
impl RoundRepository for StubRoundRepository {
    async fn get_round(&self, _round_id: &str) -> Option<Round> {
        None
    }

    async fn insert_round(&self, _round: &Round) -> Result<(), String> {
        Ok(())
    }
}
