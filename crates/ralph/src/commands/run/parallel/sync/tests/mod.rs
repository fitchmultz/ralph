//! Tests for parallel workspace state synchronization.
//!
//! Purpose:
//! - Tests for parallel workspace state synchronization.
//!
//! Responsibilities:
//! - Organize sync tests by behavior area.
//! - Provide shared fixtures and imports for companion sync test modules.
//!
//! Non-scope:
//! - Individual test scenarios, which live in sibling modules.
//! - Production sync implementation logic.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants:
//! - This hub stays thin; behavior-specific assertions live in companion files.
//! - Shared helpers are imported by sibling suites through `super::*`.

pub(super) use super::gitignored as sync_gitignored_impl;
pub(super) use super::{sync_ralph_state, sync_worker_bookkeeping_back_to_source};
pub(super) use crate::contracts::Config;
pub(super) use crate::testsupport::git as git_test;
pub(super) use anyhow::Result;
pub(super) use std::fs;
pub(super) use std::path::{Path, PathBuf};
pub(super) use tempfile::TempDir;

pub(super) fn build_test_resolved(
    repo_root: &Path,
    queue_path: Option<PathBuf>,
    done_path: Option<PathBuf>,
) -> crate::config::Resolved {
    let queue_path = queue_path.unwrap_or_else(|| repo_root.join(".ralph/queue.json"));
    let done_path = done_path.unwrap_or_else(|| repo_root.join(".ralph/done.json"));
    crate::config::Resolved {
        config: Config::default(),
        repo_root: repo_root.to_path_buf(),
        queue_path,
        done_path,
        id_prefix: "RQ".to_string(),
        id_width: 4,
        global_config_path: None,
        project_config_path: None,
    }
}

mod bookkeeping;
mod gitignored;
mod runtime;
