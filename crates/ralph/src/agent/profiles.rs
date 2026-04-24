//! Configuration profile helpers for resolving effective profile patches.
//!
//! Purpose:
//! - Configuration profile helpers for resolving effective profile patches.
//!
//! Responsibilities:
//! - Provide helpers to list and resolve config-defined profiles.
//!
//! Not handled here:
//! - CLI parsing (see `crate::cli` / `crate::agent::args`).
//! - Applying profiles to resolved config (see `crate::config`).
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Profile values are `AgentConfig` patches: only `Some(...)` fields override.

use crate::contracts::{self, AgentConfig};
use std::collections::{BTreeMap, BTreeSet};

pub(crate) fn all_profile_names(
    config_profiles: Option<&BTreeMap<String, AgentConfig>>,
) -> BTreeSet<String> {
    contracts::builtin_profile_names()
        .map(ToString::to_string)
        .chain(
            config_profiles
                .into_iter()
                .flat_map(|map| map.keys().cloned()),
        )
        .collect()
}

pub(crate) fn resolve_profile_patch(
    name: &str,
    config_profiles: Option<&BTreeMap<String, AgentConfig>>,
) -> Option<AgentConfig> {
    config_profiles
        .and_then(|map| map.get(name).cloned())
        .or_else(|| contracts::builtin_profile(name))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contracts::Runner;

    #[test]
    fn all_profile_names_is_empty_without_config_profiles() {
        let names = all_profile_names(None);
        assert!(names.contains("safe"));
        assert!(names.contains("power-user"));
    }

    #[test]
    fn all_profile_names_includes_config_profiles() {
        let mut config_profiles = BTreeMap::new();
        config_profiles.insert(
            "custom".to_string(),
            AgentConfig {
                runner: Some(Runner::Codex),
                ..Default::default()
            },
        );
        let names = all_profile_names(Some(&config_profiles));
        assert!(names.contains("custom"));
        assert!(names.contains("safe"));
    }

    #[test]
    fn resolve_profile_patch_returns_config_profile() {
        let mut config_profiles = BTreeMap::new();
        let custom_quick = AgentConfig {
            runner: Some(Runner::Codex),
            phases: Some(2),
            ..Default::default()
        };
        config_profiles.insert("fast-local".to_string(), custom_quick.clone());

        let resolved = resolve_profile_patch("fast-local", Some(&config_profiles)).unwrap();
        assert_eq!(resolved.runner, Some(Runner::Codex));
        assert_eq!(resolved.phases, Some(2));
    }

    #[test]
    fn resolve_profile_patch_returns_none_for_unknown() {
        let resolved = resolve_profile_patch("unknown_profile", None);
        assert!(resolved.is_none());
    }

    #[test]
    fn resolve_profile_patch_returns_builtin_profile() {
        let resolved = resolve_profile_patch("safe", None).expect("builtin profile");
        assert_eq!(
            resolved.git_publish_mode,
            Some(contracts::GitPublishMode::Off)
        );
        assert_eq!(
            resolved.claude_permission_mode,
            Some(contracts::ClaudePermissionMode::AcceptEdits)
        );
    }
}
