//! Job abstraction for mining work units. Protocol-agnostic.

use serde::{Deserialize, Serialize};

/// A mining job assigned to workers. Fields align with Stratum V1 notify params
/// but the model itself is protocol-agnostic.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Job {
    pub job_id: String,
    /// Previous block hash (32 bytes, internal representation).
    pub prev_hash: [u8; 32],
    /// Coinbase part 1 (before extranonce). Hex in SV1.
    pub coinbase_part1: Vec<u8>,
    /// Coinbase part 2 (after extranonce). Hex in SV1.
    pub coinbase_part2: Vec<u8>,
    /// Merkle branch hashes.
    pub merkle_branch: Vec<[u8; 32]>,
    /// Block version.
    pub version: u32,
    /// Difficulty bits.
    pub nbits: u32,
    /// Block time.
    pub ntime: u32,
    /// If true, miner should discard previous jobs.
    pub clean_jobs: bool,
}

impl Job {
    /// Create a placeholder job for stub/testing.
    pub fn placeholder() -> Self {
        Self {
            job_id: "0".to_string(),
            prev_hash: [0u8; 32],
            coinbase_part1: vec![0x01, 0x00, 0x00, 0x00, 0x00], // minimal placeholder
            coinbase_part2: vec![0xff, 0xff, 0xff, 0xff],
            merkle_branch: vec![],
            version: 0x20000000,
            nbits: 0x1d00ffff,
            ntime: 0,
            clean_jobs: true,
        }
    }
}
