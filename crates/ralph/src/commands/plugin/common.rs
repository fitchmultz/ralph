//! Shared helpers for plugin commands.
//!
//! Responsibilities:
//! - Resolve plugin installation roots from repo/config scope choices.
//! - Emit shared operator guidance reused by multiple plugin commands.
//!
//! Not handled here:
//! - Plugin manifest validation.
//! - Plugin file creation, copying, or deletion workflows.
//!
//! Invariants/assumptions:
//! - Project-scope plugins live under `.ralph/plugins`.
//! - Global-scope plugins live under `~/.config/ralph/plugins`.

use anyhow::{Result, anyhow};
use std::path::{Path, PathBuf};

use crate::cli::plugin::PluginScopeArg;

pub(super) fn scope_root(repo_root: &Path, scope: PluginScopeArg) -> Result<PathBuf> {
    Ok(match scope {
        PluginScopeArg::Project => repo_root.join(".ralph/plugins"),
        PluginScopeArg::Global => {
            let home = std::env::var_os("HOME")
                .ok_or_else(|| anyhow!("HOME environment variable not set"))?;
            PathBuf::from(home).join(".config/ralph/plugins")
        }
    })
}

pub(super) fn print_enable_hint(plugin_id: &str) {
    println!();
    println!("NOTE: The plugin is NOT automatically enabled.");
    println!("To enable it, add to your config:");
    println!(
        r#"  {{ "plugins": {{ "plugins": {{ "{}": {{ "enabled": true }} }} }} }}"#,
        plugin_id
    );
}
