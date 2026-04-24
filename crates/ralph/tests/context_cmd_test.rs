//! Integration test hub for `ralph context` commands.
//!
//! Purpose:
//! - Integration test hub for `ralph context` commands.
//!
//! Responsibilities:
//! - Group CLI integration coverage by `init`, `validate`, and `update` workflows.
//! - Keep shared repo/bootstrap helpers in adjacent test support only for this suite.
//! - Preserve a thin root module for focused failure locality.
//!
//! Not handled here:
//! - Interactive context wizard testing.
//! - Template rendering unit tests covered in command-local modules.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Tests run in isolated temp repositories.
//! - Shared helpers remain local to this integration suite.

#[path = "context_cmd_test/context_cmd_test_init.rs"]
mod context_cmd_test_init;
#[path = "context_cmd_test/context_cmd_test_support.rs"]
mod context_cmd_test_support;
#[path = "context_cmd_test/context_cmd_test_update.rs"]
mod context_cmd_test_update;
#[path = "context_cmd_test/context_cmd_test_validate.rs"]
mod context_cmd_test_validate;
