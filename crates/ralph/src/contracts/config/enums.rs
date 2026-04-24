//! Configuration enums for project type, git revert/publish mode, and scan prompt version.
//!
//! Purpose:
//! - Configuration enums for project type, git revert/publish mode, and scan prompt version.
//!
//! Responsibilities:
//! - Define simple enum types used across configuration.
//!
//! Not handled here:
//! - Complex config structs with merge behavior (see other config modules).
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/Assumptions:
//! - Keep behavior aligned with Ralph's canonical CLI, machine-contract, and queue semantics.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Project type classification.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ProjectType {
    #[default]
    Code,
    Docs,
}

/// Git revert mode for handling runner/supervision errors.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum GitRevertMode {
    #[default]
    Ask,
    Enabled,
    Disabled,
}

impl std::str::FromStr for GitRevertMode {
    type Err = &'static str;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_lowercase().as_str() {
            "ask" => Ok(GitRevertMode::Ask),
            "enabled" => Ok(GitRevertMode::Enabled),
            "disabled" => Ok(GitRevertMode::Disabled),
            _ => Err("git_revert_mode must be 'ask', 'enabled', or 'disabled'"),
        }
    }
}

/// Git publish mode for post-run repository changes.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum GitPublishMode {
    /// Leave the repository dirty after queue/done updates.
    #[default]
    Off,
    /// Create a local commit but do not push.
    Commit,
    /// Create a local commit and push it using Ralph's guarded push flow.
    CommitAndPush,
}

impl GitPublishMode {
    pub const fn as_str(self) -> &'static str {
        match self {
            GitPublishMode::Off => "off",
            GitPublishMode::Commit => "commit",
            GitPublishMode::CommitAndPush => "commit_and_push",
        }
    }
}

impl std::str::FromStr for GitPublishMode {
    type Err = &'static str;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_lowercase().as_str() {
            "off" => Ok(GitPublishMode::Off),
            "commit" => Ok(GitPublishMode::Commit),
            "commit_and_push" => Ok(GitPublishMode::CommitAndPush),
            _ => Err("git_publish_mode must be 'off', 'commit', or 'commit_and_push'"),
        }
    }
}

/// Scan prompt version to use for scan operations.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ScanPromptVersion {
    /// Version 1: Original rule-based scan prompts with fixed minimum task counts.
    V1,
    /// Version 2: Rubric-based scan prompts with quality-focused STOP CONDITION (default).
    #[default]
    V2,
}
