//! Stratum V1 wire protocol. TCP listener, JSON-RPC parsing, session state, domain mapping.
//!
//! Handles the full SV1 lifecycle: `mining.configure` (version-rolling), `mining.subscribe`,
//! `mining.authorize`, `mining.notify`, `mining.set_difficulty`, and `mining.submit`.
//!
//! Session loop uses `tokio::select!` to handle both miner requests and server-push job
//! updates via `tokio::sync::broadcast`. Each session receives fresh `mining.notify` messages
//! whenever the background job poller detects a new block template.
//!
//! Does NOT own balances, payouts, or rounds. Maps SV1 requests into pool_core domain commands.
//! SV2 support will be added via a separate protocol adapter crate.

pub mod domain_mapper;
pub mod messages;
pub mod notify;
pub mod server;
pub mod session;
pub mod session_state;

pub use domain_mapper::*;
pub use messages::*;
pub use notify::*;
pub use server::*;
pub use session::*;
pub use session_state::*;
