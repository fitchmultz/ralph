//! Plugin discovery (global + project).
//!
//! Responsibilities:
//! - Locate plugin manifests in well-known directories.
//! - Apply precedence: project overrides global by plugin id.
//!
//! Not handled here:
//! - Enable/disable decisions (see `registry`).
//! - Any plugin execution.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::plugins::manifest::PluginManifest;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PluginScope {
    Global,
    Project,
}

#[derive(Debug, Clone)]
pub(crate) struct DiscoveredPlugin {
    pub scope: PluginScope,
    pub plugin_dir: PathBuf,
    /// Path to the plugin.json manifest file.
    /// Kept for error messages and debugging.
    #[allow(dead_code)]
    pub manifest_path: PathBuf,
    pub manifest: PluginManifest,
}

pub(crate) fn plugin_roots(repo_root: &Path) -> Vec<(PluginScope, PathBuf)> {
    let mut roots = Vec::new();

    // Project: <repo>/.ralph/plugins
    roots.push((PluginScope::Project, repo_root.join(".ralph/plugins")));

    // Global: ~/.config/ralph/plugins
    if let Some(home) = std::env::var_os("HOME") {
        roots.push((
            PluginScope::Global,
            PathBuf::from(home).join(".config/ralph/plugins"),
        ));
    }

    roots
}

/// Discover plugins; project plugins override global plugins by id.
pub(crate) fn discover_plugins(
    repo_root: &Path,
) -> anyhow::Result<BTreeMap<String, DiscoveredPlugin>> {
    let mut by_id: BTreeMap<String, DiscoveredPlugin> = BTreeMap::new();

    for (scope, root) in plugin_roots(repo_root) {
        if !root.is_dir() {
            continue;
        }
        for entry in std::fs::read_dir(&root)? {
            let entry = entry?;
            let plugin_dir = entry.path();
            if !plugin_dir.is_dir() {
                continue;
            }
            let manifest_path = plugin_dir.join("plugin.json");
            if !manifest_path.is_file() {
                continue;
            }
            let raw = std::fs::read_to_string(&manifest_path)?;
            let manifest: PluginManifest = serde_json::from_str(&raw)?;
            manifest.validate()?;

            let id = manifest.id.clone();

            let discovered = DiscoveredPlugin {
                scope,
                plugin_dir,
                manifest_path,
                manifest,
            };

            // Precedence: Project overrides Global.
            match (by_id.get(&id).map(|d| d.scope), scope) {
                (Some(PluginScope::Project), PluginScope::Global) => {}
                _ => {
                    by_id.insert(id, discovered);
                }
            }
        }
    }

    Ok(by_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;
    use std::io::Write;
    use tempfile::TempDir;

    fn write_manifest(dir: &Path, id: &str) -> anyhow::Result<()> {
        write_manifest_named(dir, id, &format!("Plugin {}", id))
    }

    fn write_manifest_named(dir: &Path, id: &str, display_name: &str) -> anyhow::Result<()> {
        let manifest = crate::plugins::manifest::PluginManifest {
            api_version: super::super::PLUGIN_API_VERSION,
            id: id.to_string(),
            version: "1.0.0".to_string(),
            name: display_name.to_string(),
            description: None,
            runner: Some(crate::plugins::manifest::RunnerPlugin {
                bin: "runner".to_string(),
                supports_resume: None,
                default_model: None,
            }),
            processors: None,
        };
        let path = dir.join("plugin.json");
        let mut file = std::fs::File::create(&path)?;
        file.write_all(serde_json::to_string_pretty(&manifest)?.as_bytes())?;
        Ok(())
    }

    #[test]
    fn discover_finds_nothing_in_empty_repo() {
        let tmp = TempDir::new().unwrap();
        let discovered = discover_plugins(tmp.path()).unwrap();
        assert!(discovered.is_empty());
    }

    #[test]
    fn discover_finds_project_plugin() {
        let tmp = TempDir::new().unwrap();
        let plugin_dir = tmp.path().join(".ralph/plugins/my.plugin");
        std::fs::create_dir_all(&plugin_dir).unwrap();
        write_manifest(&plugin_dir, "my.plugin").unwrap();

        let discovered = discover_plugins(tmp.path()).unwrap();
        assert_eq!(discovered.len(), 1);
        assert!(discovered.contains_key("my.plugin"));
        assert_eq!(
            discovered.get("my.plugin").unwrap().scope,
            PluginScope::Project
        );
    }

    #[test]
    #[serial]
    fn project_overrides_global() {
        let tmp = TempDir::new().unwrap();
        let fake_home = tmp.path().join("home");
        let global_plugin = fake_home.join(".config/ralph/plugins/shared.plugin");
        std::fs::create_dir_all(&global_plugin).unwrap();
        write_manifest_named(&global_plugin, "shared.plugin", "global-plugin").unwrap();

        let project_plugin = tmp.path().join(".ralph/plugins/shared.plugin");
        std::fs::create_dir_all(&project_plugin).unwrap();
        write_manifest_named(&project_plugin, "shared.plugin", "project-plugin").unwrap();

        let original_home = std::env::var_os("HOME");
        let fake_home_str = fake_home.to_str().expect("tempdir path is utf-8");
        // SAFETY: Mutates process-global environment. `#[serial]` matches crate convention
        // for HOME tests so this does not run concurrently with other tests that touch `HOME`.
        unsafe {
            std::env::set_var("HOME", fake_home_str);
        }

        let discovered = discover_plugins(tmp.path()).unwrap();

        if let Some(h) = original_home.as_ref() {
            // SAFETY: paired restore after `set_var` above; still under `#[serial]`.
            unsafe {
                std::env::set_var("HOME", h);
            }
        } else {
            unsafe {
                std::env::remove_var("HOME");
            }
        }

        assert_eq!(discovered.len(), 1);
        let got = discovered.get("shared.plugin").unwrap();
        assert_eq!(got.scope, PluginScope::Project);
        assert_eq!(got.manifest.name, "project-plugin");
    }
}
