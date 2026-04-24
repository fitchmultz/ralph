//! Queue validation scenario coverage.
//!
//! Purpose:
//! - Queue validation scenario coverage.
//!
//! Responsibilities:
//! - Include the extracted queue validation suite in the original test module scope.
//!
//! Scope:
//! - Limited to this file's owning feature boundary.
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/Assumptions:
//! - Keep behavior aligned with Ralph's canonical CLI, machine-contract, and queue semantics.

#[path = "../validation_runtime_tests.rs"]
mod validation_runtime_tests;
