//! Webhook unit tests.
//!
//! Purpose:
//! - Webhook unit tests.
//!
//! Responsibilities:
//! - Organize webhook config, diagnostics, replay, and dispatcher tests into focused modules.
//! - Share common fixture builders and reset helpers across webhook test groups.
//!
//! Non-scope:
//! - Network delivery behavior outside the in-process test transport.
//! - Cryptographic verification beyond signature format and redaction-focused assertions.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Tests may access private module helpers via `super::*`.
//! - Shared reset helpers clear diagnostics and dispatcher state between serial tests.

mod config;
mod diagnostics;
mod support;
mod transport;
