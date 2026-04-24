//! Shared helpers for integration tests.
//!
//! Purpose:
//! - Shared helpers for integration tests.
//!
//! Responsibilities:
//! - Re-export reusable integration-test support helpers from focused submodules.
//! - Keep the root helper surface small while preserving the historical `test_support::*` API.
//! - Allow integration-test crates to opt into only the helpers they need.
//!
//! Non-scope:
//! - Defining scenario-specific fixtures or assertions for individual tests.
//! - Hiding flaky synchronization; readiness logic lives in explicit helper modules.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions callers must respect:
//! - Each integration test compiles as its own crate, so not every re-export is used everywhere.
//! - Portable temp paths must come from this module instead of hardcoded platform-specific roots.
//! - Filesystem and process helpers are test-only and may panic when required test fixtures are missing.
#![allow(dead_code, unused_imports)]

#[path = "test_support/test_support_command.rs"]
mod test_support_command;
#[path = "test_support/test_support_config.rs"]
mod test_support_config;
#[path = "test_support/test_support_fixtures.rs"]
mod test_support_fixtures;
#[path = "test_support/test_support_parallel.rs"]
mod test_support_parallel;
#[path = "test_support/test_support_path.rs"]
mod test_support_path;
#[path = "test_support/test_support_queue.rs"]
mod test_support_queue;
#[path = "test_support/test_support_snapshot.rs"]
mod test_support_snapshot;
#[path = "test_support/test_support_sync.rs"]
mod test_support_sync;

pub use test_support_command::*;
pub use test_support_config::*;
pub use test_support_fixtures::*;
pub use test_support_parallel::*;
pub use test_support_path::*;
pub use test_support_queue::*;
pub use test_support_snapshot::*;
pub use test_support_sync::*;
