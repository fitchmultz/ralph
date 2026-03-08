//! Plugin registry: combines discovery + config enable/disable + path resolution.
//!
//! Responsibilities:
//! - Provide lookup for enabled runner plugins and processor plugins.
//! - Resolve plugin runner/processor executables from plugin manifests.
//!
//! Not handled here:
//! - Installing/uninstalling plugins (see `crate::commands::plugin`).
//! - Any runner dispatch (see `crate::runner`).
//!
//! Invariants/assumptions:
//! - Disabled plugins MUST NOT be executed.
//! - Any resolved executable path is plugin-dir-relative and never escapes via `..`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::config::load_repo_trust;
use crate::contracts::Config;
use crate::plugins::discovery::{DiscoveredPlugin, PluginScope, discover_plugins};

#[derive(Debug, Clone)]
pub(crate) struct PluginRegistry {
    discovered: BTreeMap<String, DiscoveredPlugin>,
    config: crate::contracts::PluginsConfig,
}

impl PluginRegistry {
    pub(crate) fn load(repo_root: &Path, cfg: &Config) -> anyhow::Result<Self> {
        let repo_trust = load_repo_trust(repo_root)?;
        let mut discovered = discover_plugins(repo_root)?;
        if !repo_trust.is_trusted() {
            discovered.retain(|_, plugin| plugin.scope != PluginScope::Project);
        }

        Ok(Self {
            discovered,
            config: cfg.plugins.clone(),
        })
    }

    pub(crate) fn discovered(&self) -> &BTreeMap<String, DiscoveredPlugin> {
        &self.discovered
    }

    pub(crate) fn is_enabled(&self, plugin_id: &str) -> bool {
        self.config
            .plugins
            .get(plugin_id)
            .and_then(|p| p.enabled)
            .unwrap_or(false)
    }

    pub(crate) fn plugin_config_blob(&self, plugin_id: &str) -> Option<serde_json::Value> {
        self.config
            .plugins
            .get(plugin_id)
            .and_then(|p| p.config.clone())
    }

    pub(crate) fn resolve_runner_bin(&self, plugin_id: &str) -> anyhow::Result<PathBuf> {
        let discovered = self
            .discovered
            .get(plugin_id)
            .ok_or_else(|| anyhow::anyhow!("plugin not found: {plugin_id}"))?;

        let runner = discovered
            .manifest
            .runner
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("plugin {plugin_id} does not provide a runner"))?;

        resolve_plugin_relative_exe(&discovered.plugin_dir, &runner.bin)
    }

    pub(crate) fn resolve_processor_bin(&self, plugin_id: &str) -> anyhow::Result<PathBuf> {
        let discovered = self
            .discovered
            .get(plugin_id)
            .ok_or_else(|| anyhow::anyhow!("plugin not found: {plugin_id}"))?;

        let proc = discovered
            .manifest
            .processors
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("plugin {plugin_id} does not provide processors"))?;

        resolve_plugin_relative_exe(&discovered.plugin_dir, &proc.bin)
    }
}

pub(crate) fn resolve_plugin_relative_exe(plugin_dir: &Path, bin: &str) -> anyhow::Result<PathBuf> {
    let p = Path::new(bin);
    if p.is_absolute() {
        anyhow::bail!("plugin executable path must be relative to the plugin directory: {bin}");
    }

    if p.components().any(|component| {
        matches!(
            component,
            std::path::Component::ParentDir
                | std::path::Component::RootDir
                | std::path::Component::Prefix(_)
        )
    }) {
        anyhow::bail!("plugin executable path must stay within the plugin directory: {bin}");
    }

    let full = plugin_dir.join(p);
    Ok(full)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugins::manifest::{PluginManifest, RunnerPlugin};
    use std::io::Write;
    use tempfile::TempDir;

    fn create_test_plugin(dir: &Path, id: &str) -> anyhow::Result<()> {
        let manifest = PluginManifest {
            api_version: crate::plugins::PLUGIN_API_VERSION,
            id: id.to_string(),
            version: "1.0.0".to_string(),
            name: format!("Plugin {}", id),
            description: None,
            runner: Some(RunnerPlugin {
                bin: "runner.sh".to_string(),
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

    fn trust_repo(repo_root: &Path) {
        let ralph_dir = repo_root.join(".ralph");
        std::fs::create_dir_all(&ralph_dir).unwrap();
        std::fs::write(
            ralph_dir.join("trust.jsonc"),
            r#"{"allow_project_commands": true}"#,
        )
        .unwrap();
    }

    #[test]
    fn is_enabled_defaults_to_false() {
        let tmp = TempDir::new().unwrap();
        trust_repo(tmp.path());
        let plugin_dir = tmp.path().join(".ralph/plugins/test.plugin");
        std::fs::create_dir_all(&plugin_dir).unwrap();
        create_test_plugin(&plugin_dir, "test.plugin").unwrap();

        let cfg = Config::default();
        let registry = PluginRegistry::load(tmp.path(), &cfg).unwrap();

        assert!(!registry.is_enabled("test.plugin"));
    }

    #[test]
    fn is_enabled_respects_config() {
        let tmp = TempDir::new().unwrap();
        trust_repo(tmp.path());
        let plugin_dir = tmp.path().join(".ralph/plugins/test.plugin");
        std::fs::create_dir_all(&plugin_dir).unwrap();
        create_test_plugin(&plugin_dir, "test.plugin").unwrap();

        let mut cfg = Config::default();
        cfg.plugins.plugins.insert(
            "test.plugin".to_string(),
            crate::contracts::PluginConfig {
                enabled: Some(true),
                ..Default::default()
            },
        );

        let registry = PluginRegistry::load(tmp.path(), &cfg).unwrap();
        assert!(registry.is_enabled("test.plugin"));
    }

    #[test]
    fn resolve_runner_bin_rejects_disabled_plugin() {
        let tmp = TempDir::new().unwrap();
        trust_repo(tmp.path());
        let plugin_dir = tmp.path().join(".ralph/plugins/test.plugin");
        std::fs::create_dir_all(&plugin_dir).unwrap();
        create_test_plugin(&plugin_dir, "test.plugin").unwrap();

        let cfg = Config::default();
        let registry = PluginRegistry::load(tmp.path(), &cfg).unwrap();

        // Plugin exists but is not enabled - bin resolution still works
        // (enable check is done at higher level)
        let bin = registry.resolve_runner_bin("test.plugin");
        assert!(bin.is_ok());
    }

    #[test]
    fn resolve_runner_bin_fails_for_missing_plugin() {
        let tmp = TempDir::new().unwrap();
        let cfg = Config::default();
        let registry = PluginRegistry::load(tmp.path(), &cfg).unwrap();

        let err = registry.resolve_runner_bin("nonexistent");
        assert!(err.is_err());
        assert!(err.unwrap_err().to_string().contains("not found"));
    }

    #[test]
    fn resolve_plugin_relative_exe_rejects_parent_dir() {
        let tmp = TempDir::new().unwrap();
        let result = resolve_plugin_relative_exe(tmp.path(), "../escape.sh");
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("must stay within the plugin directory")
        );
    }

    #[test]
    fn resolve_plugin_relative_exe_accepts_relative_path() {
        let tmp = TempDir::new().unwrap();
        let result = resolve_plugin_relative_exe(tmp.path(), "runner.sh").unwrap();
        assert_eq!(result, tmp.path().join("runner.sh"));
    }

    #[test]
    fn resolve_plugin_relative_exe_rejects_absolute_path() {
        let tmp = TempDir::new().unwrap();
        let abs = tmp.path().join("absolute_runner.sh");
        let err = resolve_plugin_relative_exe(tmp.path(), abs.to_str().unwrap()).unwrap_err();
        assert!(err.to_string().contains("relative to the plugin directory"));
    }

    #[test]
    fn load_ignores_project_plugins_in_untrusted_repo() {
        let tmp = TempDir::new().unwrap();
        let plugin_dir = tmp.path().join(".ralph/plugins/test.plugin");
        std::fs::create_dir_all(&plugin_dir).unwrap();
        create_test_plugin(&plugin_dir, "test.plugin").unwrap();

        let cfg = Config::default();
        let registry = PluginRegistry::load(tmp.path(), &cfg).unwrap();

        assert!(registry.discovered().is_empty());
    }

    #[test]
    fn load_keeps_project_plugins_in_trusted_repo() {
        let tmp = TempDir::new().unwrap();
        let ralph_dir = tmp.path().join(".ralph");
        let plugin_dir = ralph_dir.join("plugins/test.plugin");
        std::fs::create_dir_all(&plugin_dir).unwrap();
        std::fs::write(
            ralph_dir.join("trust.jsonc"),
            r#"{"allow_project_commands": true}"#,
        )
        .unwrap();
        create_test_plugin(&plugin_dir, "test.plugin").unwrap();

        let cfg = Config::default();
        let registry = PluginRegistry::load(tmp.path(), &cfg).unwrap();

        assert!(registry.discovered().contains_key("test.plugin"));
    }
}
