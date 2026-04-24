//! Purpose: facade re-exporting parallel status and retry commands.
//!
//! Responsibilities: expose stable re-exports for status, retry, and the machine-readable status document.
//!
//! Scope: command re-exports only.
//!
//! Usage: imported by `commands/run/mod.rs` and machine clients.
//!
//! Not handled here: status inspection, rendering, or retry mutation logic.
//!
//! Invariants/assumptions: facade exports remain stable for CLI and machine clients.

#[path = "parallel/retry.rs"]
mod retry;
#[path = "parallel/status.rs"]
mod status;
#[path = "parallel/status_render.rs"]
mod status_render;
#[path = "parallel/status_support.rs"]
mod status_support;

pub(crate) use retry::parallel_retry;
pub(crate) use status::build_parallel_status_document;
pub(crate) use status::parallel_status;
