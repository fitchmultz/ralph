//! Release/public-readiness/build integration contract tests.
//!
//! Responsibilities:
//! - Guard shared contracts between Xcode, shell scripts, and the Makefile.
//! - Ensure public-readiness and bundling logic stays centralized.
//!
//! Not handled here:
//! - End-to-end release execution.
//! - Credentialed crates.io or GitHub interactions.
//!
//! Invariants/assumptions:
//! - Contract files live at stable repo-relative paths.

#[path = "release_integration_contract_test/support.rs"]
mod support;

#[path = "release_integration_contract_test/pre_public_check_contracts_early.rs"]
mod pre_public_check_contracts_early;

#[path = "release_integration_contract_test/pre_public_check_contracts_mid.rs"]
mod pre_public_check_contracts_mid;

#[path = "release_integration_contract_test/pre_public_check_contracts_tracked_paths.rs"]
mod pre_public_check_contracts_tracked_paths;

#[path = "release_integration_contract_test/pre_public_check_contracts_ralph_cleanliness.rs"]
mod pre_public_check_contracts_ralph_cleanliness;

#[path = "release_integration_contract_test/release_policy_contracts.rs"]
mod release_policy_contracts;

#[path = "release_integration_contract_test/release_bundle_contracts.rs"]
mod release_bundle_contracts;

#[path = "release_integration_contract_test/public_readiness_scan_contracts.rs"]
mod public_readiness_scan_contracts;
