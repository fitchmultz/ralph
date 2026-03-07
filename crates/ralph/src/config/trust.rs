//! Repo-local execution trust loading.
//!
//! Responsibilities:
//! - Define the local trust file contract for execution-sensitive project settings.
//! - Load `.ralph/trust.jsonc` / `.ralph/trust.json` files with JSONC support.
//! - Provide helpers for source-aware trust checks during config resolution.
//!
//! Not handled here:
//! - Main config layering or schema generation (see `crate::contracts::config`).
//! - CI command validation or execution (see `crate::config::validation` and `crate::runutil`).
//!
//! Invariants/assumptions:
//! - Trust is local-only and must not be committed to version control.
//! - Missing trust files mean the repo is untrusted.

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

use super::resolution::prefer_jsonc_then_json;

/// Local trust file for execution-sensitive project configuration.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct RepoTrust {
    /// Allow repo-local executable configuration such as `agent.ci_gate`.
    pub allow_project_commands: bool,

    /// Timestamp for the explicit trust decision.
    pub trusted_at: Option<DateTime<Utc>>,
}

impl RepoTrust {
    pub fn is_trusted(&self) -> bool {
        self.allow_project_commands
    }
}

/// Preferred local trust path for a repository root.
pub fn project_trust_path(repo_root: &Path) -> PathBuf {
    prefer_jsonc_then_json(repo_root.join(".ralph").join("trust.jsonc"))
}

/// Load repo trust if present, otherwise return the default untrusted state.
pub fn load_repo_trust(repo_root: &Path) -> Result<RepoTrust> {
    let path = project_trust_path(repo_root);
    if !path.exists() {
        return Ok(RepoTrust::default());
    }

    let raw = fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
    crate::jsonc::parse_jsonc::<RepoTrust>(&raw, &format!("trust {}", path.display()))
}
