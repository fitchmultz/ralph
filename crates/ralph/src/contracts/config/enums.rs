//! Configuration enums for project type, git revert mode, and scan prompt version.
//!
//! Responsibilities:
//! - Define simple enum types used across configuration.
//!
//! Not handled here:
//! - Complex config structs with merge behavior (see other config modules).

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
