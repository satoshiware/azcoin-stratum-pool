//! AZCOIN/Bitcoin-compatible share validation. Reconstructs block header,
//! computes double SHA256, compares against pool and block targets.

use pool_core::{Job, ShareResult, ShareSubmission, ShareValidator};
use sha2::{Digest, Sha256};
use tracing::info;

/// Pool difficulty for share target.
const DEFAULT_POOL_DIFFICULTY: u32 = 4;
const DIFF1_NBITS: u32 = 0x1d00ffff;

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

        let trace = match build_share_validation_trace(job, share, extranonce1, diff) {
            Ok(trace) => trace,
            Err(reason) => return ShareResult::Rejected { reason },
        };
        info!(
            worker = %share.worker.id,
            job_id = %share.job_id,
            extranonce1 = %hex::encode(extranonce1),
            extranonce2 = %hex::encode(&share.extra_nonce2),
            ntime = %format!("{:08x}", share.ntime),
            nonce = %format!("{:08x}", share.nonce),
            version_bits = ?trace.version_bits_hex,
            version_rolling_mask = ?trace.version_rolling_mask_hex,
            base_job_version = %format!("{:08x}", job.version),
            merged_header_version = %format!("{:08x}", trace.merged_version),
            prev_hash = %hex::encode(job.prev_hash),
            merkle_branch_len = job.merkle_branch.len(),
            merkle_branch = ?trace.merkle_branch_hex,
            coinbase_hash = %hex::encode(trace.coinbase_hash),
            validator_merkle_root = %hex::encode(trace.validator_merkle_root),
            branch_merkle_root = ?trace.branch_merkle_root_hex,
            final_header_hex = %hex::encode(&trace.header),
            share_hash = %hex::encode(trace.hash),
            block_target = %hex::encode(trace.block_target),
            share_target = %hex::encode(trace.share_target),
            target_comparison_endianness = "big-endian normalized",
            "share validation trace"
        );

        if leq_be(&trace.hash, &trace.block_target) {
            ShareResult::Block
        } else if leq_be(&trace.hash, &trace.share_target) {
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
) -> Result<Vec<u8>, String> {
    build_share_validation_trace(job, share, extranonce1, 1).map(|trace| trace.header)
}

#[derive(Debug)]
struct ShareValidationTrace {
    merged_version: u32,
    version_bits_hex: Option<String>,
    version_rolling_mask_hex: Option<String>,
    merkle_branch_hex: Vec<String>,
    coinbase_hash: [u8; 32],
    validator_merkle_root: [u8; 32],
    branch_merkle_root_hex: Option<String>,
    header: Vec<u8>,
    hash: [u8; 32],
    block_target: [u8; 32],
    share_target: [u8; 32],
}

fn build_share_validation_trace(
    job: &Job,
    share: &ShareSubmission,
    extranonce1: &[u8],
    pool_difficulty: u32,
) -> Result<ShareValidationTrace, String> {
    let coinbase = build_coinbase(job, share, extranonce1);
    let coinbase_hash = double_sha256(&coinbase);
    let validator_merkle_root = apply_merkle_branch(coinbase_hash, &job.merkle_branch);
    let branch_merkle_root = (!job.merkle_branch.is_empty()).then_some(validator_merkle_root);
    let merged_version = resolved_version(job.version, share)?;
    let header = build_block_header(
        merged_version,
        &job.prev_hash,
        &validator_merkle_root,
        share.ntime,
        job.nbits,
        share.nonce,
    );
    let hash = double_sha256(&header);
    let block_target = nbits_to_target(job.nbits);
    let share_target = div_target_by_difficulty(&diff1_target(), pool_difficulty.max(1));

    Ok(ShareValidationTrace {
        merged_version,
        version_bits_hex: share
            .validation_context
            .as_ref()
            .and_then(|ctx| ctx.version_bits)
            .map(|bits| format!("{:08x}", bits)),
        version_rolling_mask_hex: share
            .validation_context
            .as_ref()
            .and_then(|ctx| ctx.version_rolling_mask)
            .map(|mask| format!("{:08x}", mask)),
        merkle_branch_hex: job
            .merkle_branch
            .iter()
            .map(hex::encode)
            .collect(),
        coinbase_hash,
        validator_merkle_root,
        branch_merkle_root_hex: branch_merkle_root.map(hex::encode),
        header,
        hash,
        block_target,
        share_target,
    })
}

fn build_coinbase(job: &Job, share: &ShareSubmission, extranonce1: &[u8]) -> Vec<u8> {
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
    coinbase
}

fn apply_merkle_branch(mut merkle_root: [u8; 32], merkle_branch: &[[u8; 32]]) -> [u8; 32] {
    for branch in merkle_branch {
        let mut data = [0u8; 64];
        data[..32].copy_from_slice(&merkle_root);
        data[32..].copy_from_slice(branch);
        merkle_root = double_sha256(&data);
    }
    merkle_root
}

fn resolved_version(job_version: u32, share: &ShareSubmission) -> Result<u32, String> {
    let Some(ctx) = share.validation_context.as_ref() else {
        return Ok(job_version);
    };
    let Some(version_bits) = ctx.version_bits else {
        return Ok(job_version);
    };
    let Some(mask) = ctx.version_rolling_mask else {
        return Err("version rolling not negotiated".to_string());
    };
    if version_bits & !mask != 0 {
        return Err(format!(
            "version_bits {:08x} outside negotiated mask {:08x}",
            version_bits, mask
        ));
    }

    Ok((job_version & !mask) | (version_bits & mask))
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

fn diff1_target() -> [u8; 32] {
    nbits_to_target(DIFF1_NBITS)
}

/// Divide target by difficulty for Stratum share semantics.
/// Bigger difficulty = smaller target = harder. Target is big-endian 256-bit.
fn div_target_by_difficulty(target: &[u8; 32], difficulty: u32) -> [u8; 32] {
    if difficulty <= 1 {
        return *target;
    }

    let divisor = difficulty as u64;
    let mut out = [0u8; 32];
    let mut remainder: u64 = 0;

    for (i, byte) in target.iter().enumerate() {
        let value = (remainder << 8) | (*byte as u64);
        out[i] = (value / divisor) as u8;
        remainder = value % divisor;
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
    fn test_diff1_share_target_differs_from_live_network_block_target() {
        let block_target = nbits_to_target(0x170fffff);
        let share_target = div_target_by_difficulty(&diff1_target(), 1);

        assert_eq!(share_target, diff1_target());
        assert_ne!(share_target, block_target);
    }

    #[test]
    fn test_higher_pool_difficulty_produces_harder_share_target() {
        let share_target_diff1 = div_target_by_difficulty(&diff1_target(), 1);
        let share_target_diff32 = div_target_by_difficulty(&diff1_target(), 32);

        assert_ne!(share_target_diff32, share_target_diff1);
        assert!(leq_be(&share_target_diff32, &share_target_diff1));
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

    #[test]
    fn test_block_target_still_comes_from_job_nbits() {
        let job = Job {
            job_id: "trace".to_string(),
            prev_hash: [0u8; 32],
            coinbase_part1: vec![0x01, 0x02],
            coinbase_part2: vec![0x03, 0x04],
            merkle_branch: vec![],
            version: 0x20000000,
            nbits: 0x170fffff,
            ntime: 0x01020304,
            clean_jobs: true,
            block_assembly: None,
        };
        let share = ShareSubmission {
            job_id: "trace".to_string(),
            worker: pool_core::WorkerIdentity::new("u.w"),
            extra_nonce2: vec![0xbb, 0xcc],
            ntime: 0x05060708,
            nonce: 0x0a0b0c0d,
            validation_context: None,
        };

        let trace = build_share_validation_trace(&job, &share, &[0xaa, 0xbb], 1).unwrap();
        assert_eq!(trace.block_target, nbits_to_target(job.nbits));
    }

    #[test]
    fn test_build_solved_block_header_applies_negotiated_version_bits() {
        let job = Job::placeholder();
        let share = ShareSubmission {
            job_id: "0".to_string(),
            worker: pool_core::WorkerIdentity::new("u.w"),
            extra_nonce2: vec![0, 0, 0, 0],
            ntime: 0,
            nonce: 0,
            validation_context: Some(pool_core::ShareValidationContext {
                expected_extra_nonce2_len: None,
                extranonce1_hex: None,
                version_rolling_mask: Some(0x1fffe000),
                version_bits: Some(0x00002000),
            }),
        };

        let header = build_solved_block_header(&job, &share, &[0, 0, 0, 0]).unwrap();
        let version = u32::from_le_bytes(header[..4].try_into().unwrap());
        assert_eq!(version, 0x20002000);
    }

    #[test]
    fn test_build_solved_block_header_rejects_version_bits_outside_mask() {
        let job = Job::placeholder();
        let share = ShareSubmission {
            job_id: "0".to_string(),
            worker: pool_core::WorkerIdentity::new("u.w"),
            extra_nonce2: vec![0, 0, 0, 0],
            ntime: 0,
            nonce: 0,
            validation_context: Some(pool_core::ShareValidationContext {
                expected_extra_nonce2_len: None,
                extranonce1_hex: None,
                version_rolling_mask: Some(0x1fffe000),
                version_bits: Some(0x20000000),
            }),
        };

        let err = build_solved_block_header(&job, &share, &[0, 0, 0, 0]).unwrap_err();
        assert!(err.contains("outside negotiated mask"));
    }

    #[test]
    fn test_share_validation_trace_keeps_empty_merkle_branch_equivalent() {
        let job = Job {
            job_id: "trace".to_string(),
            prev_hash: [0u8; 32],
            coinbase_part1: vec![0x01, 0x02],
            coinbase_part2: vec![0x03, 0x04],
            merkle_branch: vec![],
            version: 0x20000000,
            nbits: 0x1d00ffff,
            ntime: 0x01020304,
            clean_jobs: true,
            block_assembly: None,
        };
        let share = ShareSubmission {
            job_id: "trace".to_string(),
            worker: pool_core::WorkerIdentity::new("u.w"),
            extra_nonce2: vec![0xbb, 0xcc],
            ntime: 0x05060708,
            nonce: 0x0a0b0c0d,
            validation_context: None,
        };

        let trace = build_share_validation_trace(&job, &share, &[0xaa, 0xbb], 1).unwrap();
        assert_eq!(trace.coinbase_hash, trace.validator_merkle_root);
        assert_eq!(trace.branch_merkle_root_hex, None);
    }

    #[test]
    fn test_share_validation_trace_uses_branch_applied_merkle_root() {
        let job = Job {
            job_id: "trace".to_string(),
            prev_hash: [0u8; 32],
            coinbase_part1: vec![0x01, 0x02],
            coinbase_part2: vec![0x03, 0x04],
            merkle_branch: vec![[0x11; 32]],
            version: 0x20000000,
            nbits: 0x1d00ffff,
            ntime: 0x01020304,
            clean_jobs: true,
            block_assembly: None,
        };
        let share = ShareSubmission {
            job_id: "trace".to_string(),
            worker: pool_core::WorkerIdentity::new("u.w"),
            extra_nonce2: vec![0xbb, 0xcc],
            ntime: 0x05060708,
            nonce: 0x0a0b0c0d,
            validation_context: Some(pool_core::ShareValidationContext {
                expected_extra_nonce2_len: None,
                extranonce1_hex: Some("aabb".to_string()),
                version_rolling_mask: Some(0x1fffe000),
                version_bits: Some(0x00002000),
            }),
        };

        let trace = build_share_validation_trace(&job, &share, &[0xaa, 0xbb], 1).unwrap();
        assert_eq!(trace.merged_version, 0x20002000);
        assert_eq!(hex::encode(&trace.header[..4]), "00200020");
        assert_eq!(trace.version_bits_hex.as_deref(), Some("00002000"));
        assert_eq!(trace.version_rolling_mask_hex.as_deref(), Some("1fffe000"));
        assert_eq!(trace.merkle_branch_hex, vec!["11".repeat(32)]);
        assert_ne!(trace.coinbase_hash, trace.validator_merkle_root);
        let branch_merkle_root = apply_merkle_branch(trace.coinbase_hash, &job.merkle_branch);
        assert_eq!(trace.validator_merkle_root, branch_merkle_root);
        let validator_merkle_root_hex = hex::encode(trace.validator_merkle_root);
        assert_eq!(
            trace.branch_merkle_root_hex.as_deref(),
            Some(validator_merkle_root_hex.as_str())
        );
        assert_eq!(
            hex::encode(&trace.header[36..68]),
            validator_merkle_root_hex
        );
        assert_eq!(trace.block_target.len(), 32);
        assert_eq!(trace.share_target.len(), 32);
    }
}
