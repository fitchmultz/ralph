//! Canonical built-in profile definitions for Ralph configuration.
//!
//! Purpose:
//! - Canonical built-in profile definitions for Ralph configuration.
//!
//! Responsibilities:
//! - Define the reserved built-in profiles shipped with Ralph.
//! - Provide helpers for resolving and validating reserved profile names.
//!
//! Not handled here:
//! - User-configured profile loading or merging.
//! - CLI rendering of profile lists.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Built-in profiles are the single source of truth for `safe` and `power-user`.
//! - Reserved profile names must not be user-defined in config files.

use super::AgentConfig;
use crate::contracts::config::GitPublishMode;
use crate::contracts::runner::{
    ClaudePermissionMode, RunnerApprovalMode, RunnerCliConfigRoot, RunnerCliOptionsPatch,
};
use std::collections::BTreeMap;

const RESERVED_PROFILE_NAMES: [&str; 2] = ["power-user", "safe"];

pub(crate) fn builtin_profiles() -> BTreeMap<String, AgentConfig> {
    BTreeMap::from([
        (
            "power-user".to_string(),
            AgentConfig {
                claude_permission_mode: Some(ClaudePermissionMode::BypassPermissions),
                runner_cli: Some(RunnerCliConfigRoot {
                    defaults: RunnerCliOptionsPatch {
                        approval_mode: Some(RunnerApprovalMode::Yolo),
                        ..RunnerCliOptionsPatch::default()
                    },
                    runners: BTreeMap::new(),
                }),
                git_publish_mode: Some(GitPublishMode::CommitAndPush),
                ..AgentConfig::default()
            },
        ),
        (
            "safe".to_string(),
            AgentConfig {
                claude_permission_mode: Some(ClaudePermissionMode::AcceptEdits),
                runner_cli: Some(RunnerCliConfigRoot {
                    defaults: RunnerCliOptionsPatch {
                        approval_mode: Some(RunnerApprovalMode::Safe),
                        ..RunnerCliOptionsPatch::default()
                    },
                    runners: BTreeMap::new(),
                }),
                git_publish_mode: Some(GitPublishMode::Off),
                ..AgentConfig::default()
            },
        ),
    ])
}

pub(crate) fn builtin_profile(name: &str) -> Option<AgentConfig> {
    builtin_profiles().get(name).cloned()
}

pub(crate) fn builtin_profile_names() -> impl Iterator<Item = &'static str> {
    RESERVED_PROFILE_NAMES.into_iter()
}

pub(crate) fn is_reserved_profile_name(name: &str) -> bool {
    RESERVED_PROFILE_NAMES.contains(&name)
}
