//! Parallel-mode fixture helpers for integration tests.
//!
//! Purpose:
//! - Parallel-mode fixture helpers for integration tests.
//!
//! Responsibilities:
//! - Generate deterministic fake `gh`, merge-agent, and runner binaries.
//! - Read persisted parallel state for assertions.
//! - Keep parallel-mode fixture wiring centralized and reproducible.
//!
//! Non-scope:
//! - Generic repo setup or queue fixture creation.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions callers must respect:
//! - Fake CLI helpers are shell scripts and require a Unix-like test environment.
//! - Invocation logs are append-only and intended for direct assertion by tests.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

/// Create a fake gh CLI script that simulates PR operations.
pub fn create_fake_gh_for_parallel(dir: &Path, pr_number_start: u32) -> Result<(PathBuf, PathBuf)> {
    let bin_dir = dir.join("bin");
    std::fs::create_dir_all(&bin_dir)?;

    let counter_file = bin_dir.join("pr-counter.txt");
    std::fs::write(&counter_file, pr_number_start.to_string())?;

    let invocations_file = bin_dir.join("gh-invocations.txt");

    let counter_str = counter_file.to_string_lossy().to_string();
    let invocations_str = invocations_file.to_string_lossy().to_string();

    let script = format!(
        r#"#!/bin/bash
# Fake gh CLI for parallel mode tests

INVOCATIONS_FILE="{invocations}"
echo "$@" >> "$INVOCATIONS_FILE"

if [[ "$1" == "auth" ]] && [[ "$2" == "status" ]]; then
    echo "Logged in to github.com as test-user"
    exit 0
fi

if [[ "$1" == "pr" ]] && [[ "$2" == "create" ]]; then
    PR_NUM=$(cat "{counter}")
    echo "https://github.com/test/test/pull/$PR_NUM"
    echo $((PR_NUM + 1)) > "{counter}"
    exit 0
fi

if [[ "$1" == "pr" ]] && [[ "$2" == "view" ]]; then
    PR_NUM="$3"
    echo '{{"number":'$PR_NUM',"state":"MERGED","merged":true,"mergeStateStatus":"CLEAN","url":"https://github.com/test/test/pull/'$PR_NUM'","headRefName":"test-branch","baseRefName":"main","isDraft":false}}'
    exit 0
fi

if [[ "$1" == "pr" ]] && [[ "$2" == "merge" ]]; then
    exit 0
fi

if [[ "$1" == "api" ]]; then
    if [[ "$*" == *"/pulls/"* ]]; then
        PR_NUM=$(echo "$*" | grep -o 'pulls/[0-9]*' | cut -d'/' -f2)
        echo '{{"number":'$PR_NUM',"state":"MERGED","merged":true}}'
        exit 0
    fi
fi

echo "Unknown gh command: $@" >&2
exit 0
"#,
        invocations = invocations_str,
        counter = counter_str
    );

    let gh_path = super::test_support_command::create_executable_script(&bin_dir, "gh", &script)?;
    Ok((gh_path, invocations_file))
}

/// Create a fake merge-agent script that records invocations and exits with specified code.
pub fn create_fake_merge_agent(dir: &Path, exit_code: i32) -> Result<PathBuf> {
    let bin_dir = dir.join("bin");
    std::fs::create_dir_all(&bin_dir)?;

    let marker_file = bin_dir.join("merge-agent-invocations.txt");
    let marker_str = marker_file.to_string_lossy().to_string();

    let script = format!(
        r#"#!/bin/bash
# Fake merge-agent for parallel mode tests
echo "$@" >> {marker}
echo '{{"task_id":"test","pr_number":1,"merged":true,"message":"fake merge"}}'
exit {code}
"#,
        marker = marker_str,
        code = exit_code
    );

    super::test_support_command::create_executable_script(&bin_dir, "merge-agent-recorder", &script)
}

/// Create a fake runner that exits immediately with success.
pub fn create_noop_runner(dir: &Path, runner_name: &str) -> Result<PathBuf> {
    let bin_dir = dir.join("bin");
    std::fs::create_dir_all(&bin_dir)?;

    let script = r#"#!/bin/bash
# No-op runner for tests - exit immediately
exit 0
"#;

    super::test_support_command::create_executable_script(&bin_dir, runner_name, script)
}

/// Read parallel state file from a repo as raw JSON value.
pub fn read_parallel_state(dir: &Path) -> Result<Option<serde_json::Value>> {
    let state_path = dir.join(".ralph/cache/parallel/state.json");
    if !state_path.exists() {
        return Ok(None);
    }
    let raw = std::fs::read_to_string(&state_path).context("read parallel state")?;
    let state: serde_json::Value = serde_json::from_str(&raw).context("parse parallel state")?;
    Ok(Some(state))
}
