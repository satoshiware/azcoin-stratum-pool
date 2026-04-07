//! AZCOIN-specific integration layer.
//!
//! Daemon connectivity: `DaemonClient` (JSON-RPC for `getblocktemplate` + `submitblock`),
//! `NodeApiClient` (REST API for templates and share reporting).
//!
//! Job sourcing: `RpcJobSource` and `NodeApiJobSource` implement `pool_core::JobSource`.
//! Template mapping: converts daemon templates to `Job` with coinbase construction,
//! merkle branch computation, and block assembly data preservation.
//!
//! Share validation: `AzcoinShareValidator` reconstructs block headers and verifies
//! double-SHA256 hash against pool difficulty target.
//!
//! Block submission: `AzcoinBlockSubmitter` implements `BlockSubmitter`. Full pipeline
//! from solved header reconstruction through raw block serialization to `submitblock` RPC.

pub mod api_template_mapper;
pub mod block_submit;
pub mod block_template;
pub mod candidate_submit;
pub mod chain_config;
mod coinbase_builder;
pub mod daemon;
pub mod job_source;
pub mod node_api;
pub mod payout_client;
mod raw_block_builder;
pub mod share_validator;
pub mod template_mapper;

pub use block_submit::*;
pub use block_template::*;
pub use candidate_submit::*;
pub use chain_config::*;
pub use daemon::*;
pub use job_source::*;
pub use payout_client::*;
pub use share_validator::*;
pub use template_mapper::*;
