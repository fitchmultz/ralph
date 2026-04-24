//! Prompt digest helpers.
//!
//! Purpose:
//! - Prompt digest helpers.
//!
//! Responsibilities:
//! - Compute stable cryptographic digests for normalized prompt content.
//!
//! Not handled here:
//! - Version-file persistence or sync decisions.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Trailing whitespace is ignored before hashing.
//! - Hashes are rendered as `sha256:<hex>`.

use sha2::{Digest, Sha256};

pub(crate) fn compute_hash(content: &str) -> String {
    let normalized = content.trim_end();
    let mut hasher = Sha256::new();
    hasher.update(normalized.as_bytes());
    format!("sha256:{}", hex::encode(hasher.finalize()))
}
