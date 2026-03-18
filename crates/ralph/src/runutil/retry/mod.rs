//! Purpose: Directory-backed facade for runner retry policy and backoff helpers.
//!
//! Responsibilities:
//! - Declare the focused retry-policy, backoff, and RNG companion modules.
//! - Re-export the stable `crate::runutil::*` retry surface for callers.
//!
//! Scope:
//! - Thin facade only; retry behavior lives in companion modules.
//! - Regression coverage for retry semantics lives in `tests.rs`.
//!
//! Usage:
//! - Imported by `crate::runutil` to preserve existing retry helper call sites.
//!
//! Invariants/Assumptions:
//! - Re-exported retry types and functions keep their existing signatures.
//! - Companion modules remain private to the retry boundary.

mod backoff;
mod policy;
mod rng;

#[cfg(test)]
mod tests;

pub(crate) use backoff::{FixedBackoffSchedule, compute_backoff, format_duration};
pub(crate) use policy::RunnerRetryPolicy;
pub(crate) use rng::SeededRng;
