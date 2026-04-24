//! Supervision scenario coverage.
//!
//! Purpose:
//! - Supervision scenario coverage.
//!
//! Responsibilities:
//! - Register extracted supervision runtime coverage through a thin root module.
//!
//! Scope:
//! - Limited to this file's owning feature boundary.
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/Assumptions:
//! - Keep behavior aligned with Ralph's canonical CLI, machine-contract, and queue semantics.

#[path = "runtime_tests.rs"]
mod runtime_tests;
