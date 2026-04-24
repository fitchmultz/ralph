//! Parsing and status derivation for GitHub PR helpers.
//!
//! Purpose:
//! - Parsing and status derivation for GitHub PR helpers.
//!
//! Responsibilities:
//! - Decode `gh` JSON payloads into typed PR models.
//! - Convert raw view payloads into merge/lifecycle summaries.
//! - Keep fallback detection and payload validation centralized.
//!
//! Not handled here:
//! - Running `gh` commands.
//! - Command construction for create/merge/view operations.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Missing lifecycle state is treated as `UNKNOWN`.
//! - Empty `nameWithOwner` payloads are rejected.

use anyhow::{Context, Result, anyhow, bail};

use super::types::{
    MergeState, PrInfo, PrLifecycle, PrLifecycleStatus, PrMergeStatus, PrViewJson,
    RepoViewNameWithOwnerJson,
};

pub(super) fn parse_name_with_owner_from_repo_view_json(payload: &[u8]) -> Result<String> {
    let repo: RepoViewNameWithOwnerJson =
        serde_json::from_slice(payload).context("parse gh repo view json")?;
    let trimmed = repo.name_with_owner.trim();
    if trimmed.is_empty() {
        bail!("gh repo view returned empty nameWithOwner");
    }
    Ok(trimmed.to_string())
}

pub(super) fn parse_pr_view_json(payload: &[u8]) -> Result<PrViewJson> {
    serde_json::from_slice(payload).context("parse gh pr view json")
}

pub(super) fn pr_info_from_view(json: PrViewJson) -> Result<PrInfo> {
    let number = json
        .number
        .ok_or_else(|| anyhow!("Missing PR number in gh response"))?;
    let url = json
        .url
        .ok_or_else(|| anyhow!("Missing PR url in gh response"))?;
    let head = json
        .head
        .ok_or_else(|| anyhow!("Missing PR head in gh response"))?;
    let base = json
        .base
        .ok_or_else(|| anyhow!("Missing PR base in gh response"))?;

    Ok(PrInfo {
        number,
        url,
        head,
        base,
    })
}

pub(super) fn pr_merge_status_from_view(json: &PrViewJson) -> PrMergeStatus {
    let merge_state = match json.merge_state_status.as_str() {
        "CLEAN" => MergeState::Clean,
        "DIRTY" => MergeState::Dirty,
        other => MergeState::Other(other.to_string()),
    };
    PrMergeStatus {
        merge_state,
        is_draft: json.is_draft.unwrap_or(false),
    }
}

pub(super) fn pr_lifecycle_status_from_view(json: &PrViewJson) -> PrLifecycleStatus {
    let state = json.state.as_deref().unwrap_or("UNKNOWN");
    let merged_flag = json.is_merged.unwrap_or(false) || json.merged_at.as_ref().is_some();

    let lifecycle = match state {
        "OPEN" => PrLifecycle::Open,
        "CLOSED" => {
            if merged_flag {
                PrLifecycle::Merged
            } else {
                PrLifecycle::Closed
            }
        }
        "MERGED" => PrLifecycle::Merged,
        other => PrLifecycle::Unknown(other.to_string()),
    };

    PrLifecycleStatus {
        is_merged: merged_flag || matches!(lifecycle, PrLifecycle::Merged),
        lifecycle,
    }
}

pub(super) fn should_fallback_to_merged_at(error: &anyhow::Error) -> bool {
    error.to_string().contains("Unknown JSON field: \"merged\"")
}
