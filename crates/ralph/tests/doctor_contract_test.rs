//! Contract tests for `ralph doctor` output and diagnostics.
//!
//! Purpose:
//! - Contract tests for `ralph doctor` output and diagnostics.
//!
//! Responsibilities:
//! - Provide shared command helpers for doctor contract suites.
//! - Split baseline, runner-binary, JSON, and auto-fix behavior into focused modules.
//! - Re-export suite-local bootstrap helpers so child modules avoid repeated setup.
//!
//! Not handled here:
//! - Production doctor logic or check implementation.
//! - Generic integration-test helpers shared outside this suite.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Child modules use cached seeded fixtures unless a test intentionally needs a git-only repo.
//! - `ralph_cmd_in_dir()` delegates to shared test support for environment isolation.

use std::path::Path;
use std::process::Command;

#[path = "doctor_contract_test/support.rs"]
mod support;
mod test_support;

pub(crate) use anyhow::Result;
pub(crate) use support::{
    setup_doctor_repo, setup_git_repo, setup_trusted_doctor_repo, write_global_config,
    write_repo_config,
};

/// Create a ralph command scoped to the given directory.
fn ralph_cmd_in_dir(dir: &Path) -> Command {
    test_support::ralph_command(dir)
}

#[path = "doctor_contract_test/auto_fix.rs"]
mod auto_fix;
#[path = "doctor_contract_test/baseline.rs"]
mod baseline;
#[path = "doctor_contract_test/json_output.rs"]
mod json_output;
#[path = "doctor_contract_test/repo_hygiene.rs"]
mod repo_hygiene;
#[path = "doctor_contract_test/runner_binaries.rs"]
mod runner_binaries;
