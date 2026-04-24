//! Plugin list command rendering.
//!
//! Purpose:
//! - Plugin list command rendering.
//!
//! Responsibilities:
//! - Load discovered plugins from the registry.
//! - Render plugin inventory in JSON or human-readable text.
//!
//! Not handled here:
//! - Plugin installation, validation, or scaffolding.
//! - CLI parsing or top-level command dispatch.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Discovery order comes from the registry snapshot.
//! - Human-readable output must preserve current operator-facing wording.

use anyhow::Result;
use std::collections::BTreeMap;

use crate::config::Resolved;
use crate::plugins::discovery::{PluginScope, plugin_roots};
use crate::plugins::registry::PluginRegistry;

pub(super) fn run_list(resolved: &Resolved, json_output: bool) -> Result<()> {
    let registry = PluginRegistry::load(&resolved.repo_root, &resolved.config)?;
    let discovered = registry.discovered();

    if json_output {
        let output: BTreeMap<String, serde_json::Value> = discovered
            .iter()
            .map(|(id, discovered_plugin)| {
                let enabled = registry.is_enabled(id);
                let info = serde_json::json!({
                    "id": id,
                    "name": discovered_plugin.manifest.name,
                    "version": discovered_plugin.manifest.version,
                    "scope": scope_label(discovered_plugin.scope),
                    "enabled": enabled,
                    "has_runner": discovered_plugin.manifest.runner.is_some(),
                    "has_processors": discovered_plugin.manifest.processors.is_some(),
                });
                (id.clone(), info)
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&output)?);
        return Ok(());
    }

    if discovered.is_empty() {
        println!("No plugins discovered.");
        println!();
        println!("Plugin directories checked:");
        for (scope, root) in plugin_roots(&resolved.repo_root) {
            println!("  [{}] {}", scope_label(scope), root.display());
        }
        return Ok(());
    }

    println!("Discovered plugins:");
    println!();
    for (id, discovered_plugin) in discovered.iter() {
        let enabled = registry.is_enabled(id);
        let status = if enabled { "enabled" } else { "disabled" };

        println!("  {} ({})", id, discovered_plugin.manifest.version);
        println!("    Name:    {}", discovered_plugin.manifest.name);
        println!("    Scope:   {}", scope_label(discovered_plugin.scope));
        println!("    Status:  {}", status);
        println!(
            "    Capabilities: {}",
            capability_summary(
                discovered_plugin.manifest.runner.is_some(),
                discovered_plugin.manifest.processors.is_some()
            )
        );
        if let Some(description) = &discovered_plugin.manifest.description {
            println!("    Description: {}", description);
        }
        println!();
    }

    println!("To enable a plugin, add to your config:");
    println!(r#"  {{ "plugins": {{ "plugins": {{ "<plugin-id>": {{ "enabled": true }} }} }} }}"#);

    Ok(())
}

fn scope_label(scope: PluginScope) -> &'static str {
    match scope {
        PluginScope::Global => "global",
        PluginScope::Project => "project",
    }
}

fn capability_summary(has_runner: bool, has_processors: bool) -> String {
    let mut capabilities = Vec::new();
    if has_runner {
        capabilities.push("runner");
    }
    if has_processors {
        capabilities.push("processors");
    }
    if capabilities.is_empty() {
        "none".to_string()
    } else {
        capabilities.join(", ")
    }
}

#[cfg(test)]
mod tests {
    use super::capability_summary;

    #[test]
    fn capability_summary_returns_none_when_plugin_has_no_capabilities() {
        assert_eq!(capability_summary(false, false), "none");
    }

    #[test]
    fn capability_summary_lists_runner_and_processors_in_stable_order() {
        assert_eq!(capability_summary(true, true), "runner, processors");
    }
}
