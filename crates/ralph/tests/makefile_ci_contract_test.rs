//! Integration test hub for Makefile CI contracts.
//!
//! Purpose:
//! - Integration test hub for Makefile CI contracts.
//!
//! Responsibilities:
//! - Group Makefile CI contract coverage by parsing, routing, docs, and cleanup behavior.
//! - Keep canonical CI constants and Makefile parsers in adjacent suite-local support.
//! - Preserve a thin root module for high-signal failures.
//!
//! Not handled here:
//! - Running the full CI gate itself.
//! - Production Makefile implementation outside contract assertions.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Canonical CI step constants live in the support module only.
//! - All suite modules read the repo Makefile from the current workspace.

#[path = "makefile_ci_contract_test/makefile_ci_contract_test_clean.rs"]
mod makefile_ci_contract_test_clean;
#[path = "makefile_ci_contract_test/makefile_ci_contract_test_contracts.rs"]
mod makefile_ci_contract_test_contracts;
#[path = "makefile_ci_contract_test/makefile_ci_contract_test_macos_visibility.rs"]
mod makefile_ci_contract_test_macos_visibility;
#[path = "makefile_ci_contract_test/makefile_ci_contract_test_parsing.rs"]
mod makefile_ci_contract_test_parsing;
#[path = "makefile_ci_contract_test/makefile_ci_contract_test_routing.rs"]
mod makefile_ci_contract_test_routing;
#[path = "makefile_ci_contract_test/makefile_ci_contract_test_support.rs"]
mod makefile_ci_contract_test_support;
