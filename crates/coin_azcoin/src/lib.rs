//! AZCOIN-specific integration. Daemon/RPC, block template, block submission, chain config.
//! Placeholder payout client. Minimal but structured for future expansion.

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
pub use share_validator::*;
pub use chain_config::*;
pub use daemon::*;
pub use job_source::*;
pub use payout_client::*;
pub use template_mapper::*;
