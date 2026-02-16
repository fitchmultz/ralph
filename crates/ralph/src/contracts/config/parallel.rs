//! Parallel run-loop configuration.
//!
//! Responsibilities:
//! - Define parallel config struct, merge behavior, and related enums.
//!
//! Not handled here:
//! - Parallel execution logic (see `crate::parallel` module).

use crate::contracts::runner::MergeRunnerConfig;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Parallel run-loop configuration.
#[derive(Debug, Clone, Serialize, Deserialize, Default, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct ParallelConfig {
    /// Number of workers to run concurrently when parallel mode is enabled.
    #[schemars(range(min = 2))]
    pub workers: Option<u8>,

    /// When to merge PRs (as created or after all tasks complete).
    pub merge_when: Option<ParallelMergeWhen>,

    /// Merge method to use for PRs.
    pub merge_method: Option<ParallelMergeMethod>,

    /// Automatically create PRs for completed tasks.
    pub auto_pr: Option<bool>,

    /// Automatically merge PRs when eligible.
    pub auto_merge: Option<bool>,

    /// Create draft PRs when a worker fails.
    pub draft_on_failure: Option<bool>,

    /// Policy for handling merge conflicts.
    pub conflict_policy: Option<ConflictPolicy>,

    /// Number of merge retries before giving up.
    #[schemars(range(min = 1))]
    pub merge_retries: Option<u8>,

    /// Root directory for parallel workspaces (relative to repo root if not absolute).
    pub workspace_root: Option<PathBuf>,

    /// Branch name prefix for parallel workers (e.g., "ralph/").
    pub branch_prefix: Option<String>,

    /// Delete branches after merge.
    pub delete_branch_on_merge: Option<bool>,

    /// Runner overrides for merge conflict resolution.
    pub merge_runner: Option<MergeRunnerConfig>,
}

impl ParallelConfig {
    pub fn merge_from(&mut self, other: Self) {
        if other.workers.is_some() {
            self.workers = other.workers;
        }
        if other.merge_when.is_some() {
            self.merge_when = other.merge_when;
        }
        if other.merge_method.is_some() {
            self.merge_method = other.merge_method;
        }
        if other.auto_pr.is_some() {
            self.auto_pr = other.auto_pr;
        }
        if other.auto_merge.is_some() {
            self.auto_merge = other.auto_merge;
        }
        if other.draft_on_failure.is_some() {
            self.draft_on_failure = other.draft_on_failure;
        }
        if other.conflict_policy.is_some() {
            self.conflict_policy = other.conflict_policy;
        }
        if other.merge_retries.is_some() {
            self.merge_retries = other.merge_retries;
        }
        if other.workspace_root.is_some() {
            self.workspace_root = other.workspace_root;
        }
        if other.branch_prefix.is_some() {
            self.branch_prefix = other.branch_prefix;
        }
        if other.delete_branch_on_merge.is_some() {
            self.delete_branch_on_merge = other.delete_branch_on_merge;
        }
        if let Some(other_merge_runner) = other.merge_runner {
            match &mut self.merge_runner {
                Some(existing) => existing.merge_from(other_merge_runner),
                None => self.merge_runner = Some(other_merge_runner),
            }
        }
    }
}

/// When to merge PRs in parallel mode.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum ParallelMergeWhen {
    #[default]
    AsCreated,
    AfterAll,
}

/// Merge method for PRs in parallel mode.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum ParallelMergeMethod {
    #[default]
    Squash,
    Merge,
    Rebase,
}

/// Policy for handling merge conflicts in parallel mode.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum ConflictPolicy {
    #[default]
    AutoResolve,
    RetryLater,
    Reject,
}
