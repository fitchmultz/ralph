//! Resolved settings and RepoPrompt override policy for parallel runs.
//!
//! Purpose:
//! - Resolved settings and RepoPrompt override policy for parallel runs.
//!
//! Responsibilities:
//! - Build `ParallelSettings` from resolved config and CLI options.
//! - Apply agent override rules specific to parallel worker processes.
//!
//! Not handled here:
//! - Workspace-root gitignore validation (see `preflight.rs`).
//! - Orchestration or worker lifecycle.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - `Resolved` reflects the active repo and merged config.
//! - Parallel workers must not inherit RepoPrompt plan/tooling modes that assume a single workspace.

use crate::agent::AgentOverrides;
use crate::config;
use crate::git;
use anyhow::Result;
use std::path::PathBuf;

/// Default push backoff intervals in milliseconds.
pub fn default_push_backoff_ms() -> Vec<u64> {
    vec![500, 2000, 5000, 10000]
}

pub(crate) struct ParallelRunOptions {
    pub max_tasks: u32,
    pub workers: u8,
    pub agent_overrides: AgentOverrides,
    pub force: bool,
}

#[allow(dead_code)]
pub(crate) struct ParallelSettings {
    pub(crate) workers: u8,
    pub(crate) workspace_root: PathBuf,
    pub(crate) max_push_attempts: u8,
    pub(crate) push_backoff_ms: Vec<u64>,
    pub(crate) workspace_retention_hours: u32,
}

pub(crate) fn resolve_parallel_settings(
    resolved: &config::Resolved,
    opts: &ParallelRunOptions,
) -> Result<ParallelSettings> {
    let cfg = &resolved.config.parallel;
    Ok(ParallelSettings {
        workers: opts.workers,
        workspace_root: git::workspace_root(&resolved.repo_root, &resolved.config),
        max_push_attempts: cfg.max_push_attempts.unwrap_or(50),
        push_backoff_ms: cfg
            .push_backoff_ms
            .clone()
            .unwrap_or_else(default_push_backoff_ms),
        workspace_retention_hours: cfg.workspace_retention_hours.unwrap_or(24),
    })
}

pub(crate) fn overrides_for_parallel_workers(
    resolved: &config::Resolved,
    overrides: &AgentOverrides,
) -> AgentOverrides {
    let repoprompt_flags =
        crate::agent::resolve_repoprompt_flags_from_overrides(overrides, resolved);
    if repoprompt_flags.plan_required || repoprompt_flags.tool_injection {
        log::warn!(
            "Parallel workers disable RepoPrompt plan/tooling instructions to keep edits in workspace clones."
        );
    }

    let mut worker_overrides = overrides.clone();
    worker_overrides.repoprompt_plan_required = Some(false);
    worker_overrides.repoprompt_tool_injection = Some(false);
    worker_overrides
}
