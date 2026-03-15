//! Plugin install and uninstall workflows.
//!
//! Responsibilities:
//! - Install plugin directories into the selected scope.
//! - Uninstall plugin directories after verifying the target looks like a plugin.
//!
//! Not handled here:
//! - Plugin scaffolding for new plugins.
//! - Plugin discovery listing or executable validation output.
//!
//! Invariants/assumptions:
//! - Install sources must be local directories containing `plugin.json`.
//! - Uninstall only removes directories that still contain `plugin.json`.

use anyhow::{Context, Result, bail};
use std::fs;
use std::path::Path;

use crate::cli::plugin::PluginScopeArg;
use crate::commands::plugin::common::{print_enable_hint, scope_root};
use crate::config::Resolved;
use crate::plugins::manifest::PluginManifest;

pub(super) fn run_install(resolved: &Resolved, source: &str, scope: PluginScopeArg) -> Result<()> {
    let source_path = Path::new(source);
    if !source_path.exists() {
        bail!("Source path does not exist: {}", source);
    }
    if !source_path.is_dir() {
        bail!("Source path is not a directory: {}", source);
    }

    let manifest_path = source_path.join("plugin.json");
    if !manifest_path.exists() {
        bail!("Source directory does not contain plugin.json: {}", source);
    }

    let manifest = load_manifest(&manifest_path)?;
    let target_root = scope_root(&resolved.repo_root, scope)?;
    let target_dir = target_root.join(&manifest.id);

    if target_dir.exists() {
        bail!(
            "Plugin {} is already installed at {}. Use uninstall first.",
            manifest.id,
            target_dir.display()
        );
    }

    fs::create_dir_all(&target_root)
        .with_context(|| format!("create plugin directory {}", target_root.display()))?;
    copy_dir_all(source_path, &target_dir)
        .with_context(|| format!("copy plugin to {}", target_dir.display()))?;

    println!(
        "Installed plugin {} to {}",
        manifest.id,
        target_dir.display()
    );
    print_enable_hint(&manifest.id);
    Ok(())
}

pub(super) fn run_uninstall(
    resolved: &Resolved,
    plugin_id: &str,
    scope: PluginScopeArg,
) -> Result<()> {
    let target_root = scope_root(&resolved.repo_root, scope)?;
    let target_dir = target_root.join(plugin_id);

    if !target_dir.exists() {
        bail!(
            "Plugin {} is not installed at {}.",
            plugin_id,
            target_dir.display()
        );
    }

    let manifest_path = target_dir.join("plugin.json");
    if !manifest_path.exists() {
        bail!(
            "Directory {} does not appear to be a plugin (no plugin.json).",
            target_dir.display()
        );
    }

    fs::remove_dir_all(&target_dir)
        .with_context(|| format!("remove plugin directory {}", target_dir.display()))?;

    println!(
        "Uninstalled plugin {} from {}",
        plugin_id,
        target_dir.display()
    );
    Ok(())
}

fn load_manifest(manifest_path: &Path) -> Result<PluginManifest> {
    let raw = fs::read_to_string(manifest_path)
        .with_context(|| format!("read {}", manifest_path.display()))?;
    let manifest: PluginManifest = serde_json::from_str(&raw).context("parse plugin.json")?;
    manifest.validate().context("validate plugin manifest")?;
    Ok(manifest)
}

fn copy_dir_all(src: &Path, dst: &Path) -> Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let destination = dst.join(entry.file_name());
        if file_type.is_dir() {
            copy_dir_all(&entry.path(), &destination)?;
        } else {
            fs::copy(entry.path(), destination)?;
        }
    }
    Ok(())
}
