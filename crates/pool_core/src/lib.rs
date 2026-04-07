//! Protocol-agnostic mining pool domain logic.
//!
//! Core domain models: `Job`, `ShareSubmission`, `ShareResult`, `WorkerIdentity`, `PoolStats`.
//! Core services: `ActiveJobRegistry` (bounded job cache with `clean_jobs` invalidation),
//! `JobAwareShareProcessor` (validates shares against registered jobs with optional crypto
//! validation), `InMemoryWorkerRegistry`, `InMemoryStatsSnapshot`.
//! Traits: `JobSource`, `ShareProcessor`, `ShareValidator`, `BlockSubmitter`, `ShareSink`.
//!
//! This crate has no dependency on any wire protocol. Protocol adapters depend on it.

pub mod balance;
pub mod block;
pub mod job;
pub mod payout;
pub mod round;
pub mod services;
pub mod session;
pub mod share;
pub mod share_buffer;
pub mod stats;
pub mod traits;
pub mod worker;

pub use balance::*;
pub use block::*;
pub use job::*;
pub use payout::*;
pub use round::*;
pub use services::*;
pub use session::*;
pub use share::*;
pub use share_buffer::*;
pub use stats::*;
pub use traits::*;
pub use worker::*;
