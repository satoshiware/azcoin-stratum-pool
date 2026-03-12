//! Block candidate for submission to the chain.

use serde::{Deserialize, Serialize};

/// A block candidate ready for submission.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockCandidate {
    pub block_hash: [u8; 32],
    pub height: u64,
    pub raw_block: Vec<u8>,
}
