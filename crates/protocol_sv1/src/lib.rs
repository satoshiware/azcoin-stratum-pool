//! Stratum V1 wire protocol. Listener, session, message types, parsing, domain mapping.
//! Does NOT own balances, payouts, or rounds. Maps SV1 requests into pool_core domain commands.
//! TODO: SV2 adapter will be added via protocol adapter layer—this crate stays SV1-focused.

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
