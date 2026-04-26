//! Public-readiness scan helper contracts (Python + shell).
//!
//! Purpose:
//! - Group public-readiness helper coverage into focused behavior modules.
//!
//! Responsibilities:
//! - Keep mode, link, secret, and CLI contracts easier to maintain.
//!
//! Scope:
//! - Limited to this file's owning feature boundary.
//!
//! Usage:
//! - Loaded by the release integration contract test harness.
//!
//! Invariants/Assumptions:
//! - Test grouping must preserve the existing Python and shell helper contract assertions.

#[path = "public_readiness_scan_contracts/modes_and_cli.rs"]
mod modes_and_cli;

#[path = "public_readiness_scan_contracts/links.rs"]
mod links;

#[path = "public_readiness_scan_contracts/secrets.rs"]
mod secrets;

#[path = "public_readiness_scan_contracts/docs_contracts.rs"]
mod docs_contracts;
