//! Plugin configuration for enable/disable and per-plugin settings.
//!
//! Responsibilities:
//! - Define plugin config structs and merge behavior for plugin activation.
//!
//! Not handled here:
//! - Plugin loading and execution (see `crate::plugin` module).

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Plugin configuration container.
#[derive(Debug, Clone, Serialize, Deserialize, Default, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct PluginsConfig {
    /// Per-plugin configuration keyed by plugin id.
    pub plugins: BTreeMap<String, PluginConfig>,
}

impl PluginsConfig {
    pub fn merge_from(&mut self, other: Self) {
        for (id, patch) in other.plugins {
            self.plugins
                .entry(id)
                .and_modify(|existing| existing.merge_from(patch.clone()))
                .or_insert(patch);
        }
    }
}

/// Per-plugin configuration.
#[derive(Debug, Clone, Serialize, Deserialize, Default, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct PluginConfig {
    /// Enable/disable the plugin. If None, defaults to disabled.
    pub enabled: Option<bool>,

    /// Opaque plugin configuration blob (passed through to the plugin).
    pub config: Option<serde_json::Value>,
}

impl PluginConfig {
    pub fn merge_from(&mut self, other: Self) {
        if other.enabled.is_some() {
            self.enabled = other.enabled;
        }
        if other.config.is_some() {
            self.config = other.config;
        }
    }
}
