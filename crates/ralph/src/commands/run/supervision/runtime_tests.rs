//! Supervision runtime test hub.
//!
//! Purpose:
//! - Supervision runtime test hub.
//!
//! Responsibilities:
//! - Group supervision runtime coverage by resume handling, post-run orchestration, and worker restore flows.
//! - Keep reusable queue/config/session builders in adjacent test-only support.
//! - Preserve a thin root entrypoint for `tests.rs`.
//!
//! Not handled here:
//! - Production supervision implementation logic.
//! - Cross-crate integration scenarios in `crates/ralph/tests/`.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Child modules stay focused on one behavior seam each.
//! - Shared runtime test builders live in `support.rs`.

#[path = "runtime_tests/ci_continue.rs"]
mod ci_continue;
#[path = "runtime_tests/parallel_worker.rs"]
mod parallel_worker;
#[path = "runtime_tests/post_run_supervise.rs"]
mod post_run_supervise;
#[path = "runtime_tests/resume_session.rs"]
mod resume_session;
#[path = "runtime_tests/session_state.rs"]
mod session_state;
#[path = "runtime_tests/support.rs"]
mod support;
