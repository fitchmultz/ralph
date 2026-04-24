//! Data types for GitHub PR helpers.
//!
//! Purpose:
//! - Data types for GitHub PR helpers.
//!
//! Responsibilities:
//! - Define public PR operation/status types consumed by the rest of the crate.
//! - Define serde-backed view payloads shared by parsing/execution helpers.
//!
//! Not handled here:
//! - Running `gh` commands.
//! - Converting raw payloads into derived status models.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - `PrViewJson` matches the subset of `gh pr view --json` fields requested by this module.

use serde::Deserialize;

/// Merge method for PRs.
/// NOTE: This is a local copy since the config version was removed in the direct-push rewrite.
/// This enum is kept for backward compatibility with existing PR operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[allow(dead_code)]
pub(crate) enum MergeMethod {
    #[default]
    Squash,
    Merge,
    Rebase,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub(crate) struct PrInfo {
    pub number: u32,
    #[allow(dead_code)]
    pub url: String,
    #[allow(dead_code)]
    pub head: String,
    #[allow(dead_code)]
    pub base: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum MergeState {
    Clean,
    Dirty,
    Other(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PrMergeStatus {
    pub merge_state: MergeState,
    pub is_draft: bool,
}

#[derive(Deserialize, Debug, Clone)]
#[allow(dead_code)]
pub(super) struct PrViewJson {
    #[serde(rename = "mergeStateStatus")]
    pub(super) merge_state_status: String,
    pub(super) number: Option<u32>,
    pub(super) url: Option<String>,
    #[serde(rename = "headRefName")]
    pub(super) head: Option<String>,
    #[serde(rename = "baseRefName")]
    pub(super) base: Option<String>,
    #[serde(rename = "isDraft")]
    pub(super) is_draft: Option<bool>,
    pub(super) state: Option<String>,
    #[serde(rename = "merged")]
    pub(super) is_merged: Option<bool>,
    #[serde(rename = "mergedAt")]
    pub(super) merged_at: Option<String>,
}

#[derive(Deserialize)]
pub(super) struct RepoViewNameWithOwnerJson {
    #[serde(rename = "nameWithOwner")]
    pub(super) name_with_owner: String,
}

/// PR lifecycle states as returned by GitHub.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PrLifecycle {
    Open,
    Closed,
    Merged,
    Unknown(String),
}

/// PR lifecycle status including lifecycle and merged flag.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PrLifecycleStatus {
    pub(super) lifecycle: PrLifecycle,
    pub(super) is_merged: bool,
}

pub(super) const PRIMARY_VIEW_FIELDS: &str =
    "mergeStateStatus,number,url,headRefName,baseRefName,isDraft,state,merged";
pub(super) const FALLBACK_VIEW_FIELDS: &str =
    "mergeStateStatus,number,url,headRefName,baseRefName,isDraft,state,mergedAt";
