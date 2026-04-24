//! Queue validation runtime test hub.
//!
//! Purpose:
//! - Queue validation runtime test hub.
//!
//! Responsibilities:
//! - Group queue validation runtime coverage by validation concern.
//! - Keep helper builders and fixtures in adjacent test-only modules.
//! - Preserve a thin root entrypoint for `validation.rs` test registration.
//!
//! Not handled here:
//! - Validation implementation logic (see `validation.rs`).
//! - Queue loading or repair integration flows outside validation coverage.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Each child module targets one validation seam.
//! - Shared task builders live in `support.rs` only for this suite.

#[path = "validation_runtime_tests/core.rs"]
mod core;
#[path = "validation_runtime_tests/dependencies.rs"]
mod dependencies;
#[path = "validation_runtime_tests/deserialization.rs"]
mod deserialization;
#[path = "validation_runtime_tests/parent.rs"]
mod parent;
#[path = "validation_runtime_tests/relationships.rs"]
mod relationships;
#[path = "validation_runtime_tests/support.rs"]
mod support;
