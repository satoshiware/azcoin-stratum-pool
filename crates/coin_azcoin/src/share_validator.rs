//! AZCOIN/Bitcoin-compatible share validation. Reconstructs block header,
//! computes double SHA256, compares against pool and block targets.

use pool_core::{Job, ShareResult, ShareSubmission, ShareValidator};
use sha2::{Digest, Sha256};

/// Pool difficulty for share target. Share target = block_target * pool_difficulty.
const DEFAULT_POOL_DIFFICULTY: u32 = 4;

/// AZCOIN share validator. Bitcoin-compatible block header hashing.
#[derive(Default)]
pub struct AzcoinShareValidator {
    #[allow(dead_code)]
    pool_difficulty: u32,
}

impl AzcoinShareValidator {
    pub fn new() -> Self {
        Self {
            pool_difficulty: DEFAULT_POOL_DIFFICULTY,
        }
    }

    pub fn with_pool_difficulty(pool_difficulty: u32) -> Self {
        Self { pool_difficulty }
    }
}

impl ShareValidator for AzcoinShareValidator {
    fn validate_share(
        &self,
        job: &Job,
        share: &ShareSubmission,
        extranonce1: &[u8],
        pool_difficulty: u32,
    ) -> ShareResult {
        let diff = pool_difficulty.max(1);

        let header = build_solved_block_header(job, share, extranonce1);

        // 4. Double SHA256 of header
        let hash = double_sha256(&header);

        // 5. Compare against targets (hash and target are big-endian 256-bit)
        let block_target = nbits_to_target(job.nbits);
        let share_target = mul_target_by_difficulty(&block_target, diff);

        if leq_be(&hash, &block_target) {
            ShareResult::Block
        } else if leq_be(&hash, &share_target) {
            ShareResult::Accepted
        } else {
            ShareResult::LowDifficulty {
                reason: format!("hash above pool target (pool difficulty {})", diff),
            }
        }
    }
}

/// Reconstruct the solved 80-byte block header from a validated share path.
pub fn build_solved_block_header(
    job: &Job,
    share: &ShareSubmission,
    extranonce1: &[u8],
) -> Vec<u8> {
    // 1. Reconstruct coinbase: part1 || extranonce1 || extranonce2 || part2
    let mut coinbase = Vec::with_capacity(
        job.coinbase_part1.len()
            + extranonce1.len()
            + share.extra_nonce2.len()
            + job.coinbase_part2.len(),
    );
    coinbase.extend_from_slice(&job.coinbase_part1);
    coinbase.extend_from_slice(extranonce1);
    coinbase.extend_from_slice(&share.extra_nonce2);
    coinbase.extend_from_slice(&job.coinbase_part2);

    // 2. Merkle root for single-coinbase block
    let merkle_root = double_sha256(&coinbase);

    // 3. Build 80-byte block header (little-endian)
    build_block_header(
        job.version,
        &job.prev_hash,
        &merkle_root,
        share.ntime,
        job.nbits,
        share.nonce,
    )
}

/// Build 80-byte Bitcoin block header: version | prev_hash | merkle_root | ntime | nbits | nonce.
fn build_block_header(
    version: u32,
    prev_hash: &[u8; 32],
    merkle_root: &[u8; 32],
    ntime: u32,
    nbits: u32,
    nonce: u32,
) -> Vec<u8> {
    let mut header = Vec::with_capacity(80);
    header.extend_from_slice(&version.to_le_bytes());
    header.extend_from_slice(prev_hash);
    header.extend_from_slice(merkle_root);
    header.extend_from_slice(&ntime.to_le_bytes());
    header.extend_from_slice(&nbits.to_le_bytes());
    header.extend_from_slice(&nonce.to_le_bytes());
    header
}

/// Double SHA256.
fn double_sha256(data: &[u8]) -> [u8; 32] {
    let first = Sha256::digest(data);
    let second = Sha256::digest(first);
    second.into()
}

/// Convert nBits to 256-bit target (big-endian for comparison).
/// nBits: first byte = exponent, last 3 bytes = mantissa. target = mantissa * 256^(exponent - 3).
fn nbits_to_target(nbits: u32) -> [u8; 32] {
    let mut target = [0u8; 32];
    let exponent = (nbits >> 24) as usize;
    let mantissa = nbits & 0xffffff;

    if exponent <= 3 {
        let v = (mantissa as u64) << (8 * (3 - exponent));
        target[28] = (v >> 24) as u8;
        target[29] = (v >> 16) as u8;
        target[30] = (v >> 8) as u8;
        target[31] = v as u8;
    } else {
        let shift = exponent - 3;
        let byte_offset = 32 - shift - 3;
        if byte_offset < 32 {
            target[byte_offset] = (mantissa >> 16) as u8;
            if byte_offset + 1 < 32 {
                target[byte_offset + 1] = (mantissa >> 8) as u8;
            }
            if byte_offset + 2 < 32 {
                target[byte_offset + 2] = mantissa as u8;
            }
        }
    }
    target
}

/// Multiply target by difficulty (share target = block_target * difficulty).
/// Bigger target = easier. Target is big-endian 256-bit.
fn mul_target_by_difficulty(target: &[u8; 32], difficulty: u32) -> [u8; 32] {
    if difficulty <= 1 {
        return *target;
    }
    let diff = difficulty as u64;
    let mut result = [0u8; 40]; // 32 + 8 for overflow
    let mut carry: u64 = 0;

    for i in (0..32).rev() {
        let v = (target[i] as u64) * diff + carry;
        result[i + 8] = v as u8;
        carry = v >> 8;
    }
    for i in (0..8).rev() {
        let v = (result[i] as u64) + carry;
        result[i] = v as u8;
        carry = v >> 8;
        if carry == 0 {
            break;
        }
    }
    let mut out = [0u8; 32];
    if carry > 0 {
        out.fill(0xff);
    } else {
        out.copy_from_slice(&result[8..40]);
    }
    out
}

/// Compare two 32-byte big-endian integers. Returns true if a <= b.
fn leq_be(a: &[u8; 32], b: &[u8; 32]) -> bool {
    for i in 0..32 {
        if a[i] < b[i] {
            return true;
        }
        if a[i] > b[i] {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_double_sha256() {
        let empty: [u8; 0] = [];
        let h = double_sha256(&empty);
        // SHA256(SHA256([])) = 0x5df6e0e2...
        assert_eq!(h.len(), 32);
    }

    #[test]
    fn test_nbits_to_target_genesis() {
        // Bitcoin genesis 0x1d00ffff: exponent=29, mantissa=0x00ffff
        // target = 0x00ffff * 256^26, 29 bytes, big-endian: [0,0,0, 0x00,0xff,0xff, 0..0]
        let t = nbits_to_target(0x1d00ffff);
        assert_eq!(t[0], 0);
        assert_eq!(t[1], 0);
        assert_eq!(t[2], 0);
        assert_eq!(t[3], 0);
        assert_eq!(t[4], 0xff);
        assert_eq!(t[5], 0xff);
        assert_eq!(t[6], 0);
        assert_eq!(t[31], 0);
    }

    #[test]
    fn test_build_block_header_len() {
        let h = build_block_header(0x20000000, &[0u8; 32], &[0u8; 32], 0, 0x1d00ffff, 0);
        assert_eq!(h.len(), 80);
    }

    #[test]
    fn test_validate_share_low_difficulty() {
        // Create a job and share that will produce a hash above pool target
        let job = pool_core::Job {
            job_id: "test".to_string(),
            prev_hash: [0u8; 32],
            coinbase_part1: vec![0x01, 0x00, 0x00, 0x00, 0x00],
            coinbase_part2: vec![0xff, 0xff, 0xff, 0xff],
            merkle_branch: vec![],
            version: 0x20000000,
            nbits: 0x1d00ffff,
            ntime: 0,
            clean_jobs: true,
            block_assembly: None,
        };
        let share = ShareSubmission {
            job_id: "test".to_string(),
            worker: pool_core::WorkerIdentity::new("u.w"),
            extra_nonce2: vec![0, 0, 0, 0],
            ntime: 0,
            nonce: 0,
            validation_context: None,
        };
        let validator = AzcoinShareValidator::new();
        let result = validator.validate_share(&job, &share, &[0, 0, 0, 0], 4);
        match &result {
            ShareResult::LowDifficulty { .. } => {}
            _ => panic!("expected LowDifficulty, got {:?}", result),
        }
    }
}
