//! Plugin command implementations.
//!
//! Responsibilities:
//! - Implement plugin list, validate, install, and uninstall commands.
//!
//! Not handled here:
//! - CLI argument parsing (see `crate::cli::plugin`).
//! - Plugin discovery/registry (see `crate::plugins`).

use anyhow::{Context, Result};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::cli::plugin::{PluginArgs, PluginCommand};
use crate::config::Resolved;
use crate::plugins::discovery::{PluginScope, discover_plugins, plugin_roots};
use crate::plugins::manifest::PluginManifest;
use crate::plugins::registry::PluginRegistry;

pub fn run(args: &PluginArgs, resolved: &Resolved) -> Result<()> {
    match &args.command {
        PluginCommand::List { json } => cmd_list(resolved, *json),
        PluginCommand::Validate { id } => cmd_validate(resolved, id.as_deref()),
        PluginCommand::Install { source, scope } => cmd_install(resolved, source, scope),
        PluginCommand::Uninstall { id, scope } => cmd_uninstall(resolved, id, scope),
    }
}

fn cmd_list(resolved: &Resolved, json_output: bool) -> Result<()> {
    let registry = PluginRegistry::load(&resolved.repo_root, &resolved.config)?;
    let discovered = registry.discovered();

    if json_output {
        let output: BTreeMap<String, serde_json::Value> = discovered
            .iter()
            .map(|(id, d)| {
                let enabled = registry.is_enabled(id);
                let info = serde_json::json!({
                    "id": id,
                    "name": d.manifest.name,
                    "version": d.manifest.version,
                    "scope": match d.scope {
                        PluginScope::Global => "global",
                        PluginScope::Project => "project",
                    },
                    "enabled": enabled,
                    "has_runner": d.manifest.runner.is_some(),
                    "has_processors": d.manifest.processors.is_some(),
                });
                (id.clone(), info)
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        if discovered.is_empty() {
            println!("No plugins discovered.");
            println!();
            println!("Plugin directories checked:");
            for (scope, root) in plugin_roots(&resolved.repo_root) {
                let scope_str = match scope {
                    PluginScope::Global => "global",
                    PluginScope::Project => "project",
                };
                println!("  [{}] {}", scope_str, root.display());
            }
            return Ok(());
        }

        println!("Discovered plugins:");
        println!();
        for (id, d) in discovered.iter() {
            let enabled = registry.is_enabled(id);
            let scope_str = match d.scope {
                PluginScope::Global => "global",
                PluginScope::Project => "project",
            };
            let status = if enabled { "enabled" } else { "disabled" };
            let capabilities = {
                let mut caps = Vec::new();
                if d.manifest.runner.is_some() {
                    caps.push("runner");
                }
                if d.manifest.processors.is_some() {
                    caps.push("processors");
                }
                if caps.is_empty() {
                    "none".to_string()
                } else {
                    caps.join(", ")
                }
            };

            println!("  {} ({})", id, d.manifest.version);
            println!("    Name:    {}", d.manifest.name);
            println!("    Scope:   {}", scope_str);
            println!("    Status:  {}", status);
            println!("    Capabilities: {}", capabilities);
            if let Some(desc) = &d.manifest.description {
                println!("    Description: {}", desc);
            }
            println!();
        }

        println!("To enable a plugin, add to your config:");
        println!(
            r#"  {{ "plugins": {{ "plugins": {{ "<plugin-id>": {{ "enabled": true }} }} }} }}"#
        );
    }

    Ok(())
}

fn cmd_validate(resolved: &Resolved, filter_id: Option<&str>) -> Result<()> {
    let discovered = discover_plugins(&resolved.repo_root)?;

    if discovered.is_empty() {
        println!("No plugins to validate.");
        return Ok(());
    }

    let mut validated = 0;
    let mut errors = 0;

    for (id, d) in discovered.iter() {
        if let Some(filter) = filter_id
            && id != filter
        {
            continue;
        }

        print!("Validating {}... ", id);

        // Manifest was already validated during discovery, but re-validate for thoroughness
        if let Err(e) = d.manifest.validate() {
            println!("FAILED (manifest)");
            println!("  Error: {}", e);
            errors += 1;
            continue;
        }

        // Check runner binary exists if specified
        if let Some(runner) = &d.manifest.runner {
            let bin_path = d.plugin_dir.join(&runner.bin);
            if !bin_path.exists() {
                println!("FAILED (runner binary)");
                println!("  Error: runner binary not found: {}", bin_path.display());
                errors += 1;
                continue;
            }
        }

        // Check processor binary exists if specified
        if let Some(proc) = &d.manifest.processors {
            let bin_path = d.plugin_dir.join(&proc.bin);
            if !bin_path.exists() {
                println!("FAILED (processor binary)");
                println!(
                    "  Error: processor binary not found: {}",
                    bin_path.display()
                );
                errors += 1;
                continue;
            }
        }

        println!("OK");
        validated += 1;
    }

    if let Some(filter) = filter_id
        && validated == 0
        && errors == 0
    {
        println!("Plugin '{}' not found.", filter);
    }

    if errors > 0 {
        anyhow::bail!("{} validation error(s) found", errors);
    }

    println!("{} plugin(s) validated successfully.", validated);
    Ok(())
}

fn cmd_install(resolved: &Resolved, source: &str, scope: &str) -> Result<()> {
    let source_path = Path::new(source);
    if !source_path.exists() {
        anyhow::bail!("Source path does not exist: {}", source);
    }
    if !source_path.is_dir() {
        anyhow::bail!("Source path is not a directory: {}", source);
    }

    // Validate manifest exists and is valid
    let manifest_path = source_path.join("plugin.json");
    if !manifest_path.exists() {
        anyhow::bail!("Source directory does not contain plugin.json: {}", source);
    }

    let manifest: PluginManifest = {
        let raw = fs::read_to_string(&manifest_path)
            .with_context(|| format!("read {}", manifest_path.display()))?;
        serde_json::from_str(&raw).context("parse plugin.json")?
    };
    manifest.validate().context("validate plugin manifest")?;

    let plugin_id = &manifest.id;

    // Determine target directory
    let target_root = match scope {
        "global" => {
            let home = std::env::var_os("HOME")
                .ok_or_else(|| anyhow::anyhow!("HOME environment variable not set"))?;
            PathBuf::from(home).join(".config/ralph/plugins")
        }
        "project" => resolved.repo_root.join(".ralph/plugins"),
        other => anyhow::bail!("Invalid scope: {}. Use 'project' or 'global'.", other),
    };

    let target_dir = target_root.join(plugin_id);

    // Check if already exists
    if target_dir.exists() {
        anyhow::bail!(
            "Plugin {} is already installed at {}. Use uninstall first.",
            plugin_id,
            target_dir.display()
        );
    }

    // Create target directory and copy plugin
    fs::create_dir_all(&target_root)
        .with_context(|| format!("create plugin directory {}", target_root.display()))?;

    // Copy directory recursively
    copy_dir_all(source_path, &target_dir)
        .with_context(|| format!("copy plugin to {}", target_dir.display()))?;

    println!("Installed plugin {} to {}", plugin_id, target_dir.display());
    println!();
    println!("NOTE: The plugin is NOT automatically enabled.");
    println!("To enable it, add to your config:");
    println!(
        r#"  {{ "plugins": {{ "plugins": {{ "{}": {{ "enabled": true }} }} }} }}"#,
        plugin_id
    );

    Ok(())
}

fn cmd_uninstall(resolved: &Resolved, plugin_id: &str, scope: &str) -> Result<()> {
    // Determine target directory
    let target_root = match scope {
        "global" => {
            let home = std::env::var_os("HOME")
                .ok_or_else(|| anyhow::anyhow!("HOME environment variable not set"))?;
            PathBuf::from(home).join(".config/ralph/plugins")
        }
        "project" => resolved.repo_root.join(".ralph/plugins"),
        other => anyhow::bail!("Invalid scope: {}. Use 'project' or 'global'.", other),
    };

    let target_dir = target_root.join(plugin_id);

    if !target_dir.exists() {
        anyhow::bail!(
            "Plugin {} is not installed at {}.",
            plugin_id,
            target_dir.display()
        );
    }

    // Verify it's actually a plugin directory
    let manifest_path = target_dir.join("plugin.json");
    if !manifest_path.exists() {
        anyhow::bail!(
            "Directory {} does not appear to be a plugin (no plugin.json).",
            target_dir.display()
        );
    }

    // Remove the directory
    fs::remove_dir_all(&target_dir)
        .with_context(|| format!("remove plugin directory {}", target_dir.display()))?;

    println!(
        "Uninstalled plugin {} from {}",
        plugin_id,
        target_dir.display()
    );

    Ok(())
}

fn copy_dir_all(src: impl AsRef<Path>, dst: impl AsRef<Path>) -> Result<()> {
    fs::create_dir_all(&dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        if ty.is_dir() {
            copy_dir_all(entry.path(), dst.as_ref().join(entry.file_name()))?;
        } else {
            fs::copy(entry.path(), dst.as_ref().join(entry.file_name()))?;
        }
    }
    Ok(())
}
