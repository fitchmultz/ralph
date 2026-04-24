//! Runner-specific command assembly for execution.
//!
//! Purpose:
//! - Runner-specific command assembly for execution.
//!
//! Responsibilities:
//! - Assemble runner-specific commands and payloads for execution.
//! - Delegate execution to the shared streaming process runner.
//!
//! Non-scope:
//! - Validating models or global runner configuration.
//! - Persisting runner output or mutating queue state.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants:
//! - Caller supplies validated model/runner inputs and a writable work dir.
//! - Command builder guards are kept alive for the duration of execution.
//!
//! This file is kept as a placeholder for any future legacy runner implementations.
//! All built-in runners have been migrated to the plugin system.
