//! Plugin configuration for enable/disable and per-plugin settings.
//!
//! Responsibilities:
//! - Define plugin config structs and merge behavior.
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

    /// Optional overrides for runner executable path/name for this plugin's runner.
    /// If not set, manifest runner.bin is used.
    pub runner: Option<PluginRunnerConfig>,

    /// Optional overrides for processor executable path/name for this plugin's task processors.
    pub processor: Option<PluginProcessorConfig>,

    /// Opaque plugin configuration blob (passed through to the plugin).
    pub config: Option<serde_json::Value>,
}

impl PluginConfig {
    pub fn merge_from(&mut self, other: Self) {
        if other.enabled.is_some() {
            self.enabled = other.enabled;
        }
        if let Some(r) = other.runner {
            match &mut self.runner {
                Some(existing) => existing.merge_from(r),
                None => self.runner = Some(r),
            }
        }
        if let Some(p) = other.processor {
            match &mut self.processor {
                Some(existing) => existing.merge_from(p),
                None => self.processor = Some(p),
            }
        }
        if other.config.is_some() {
            self.config = other.config;
        }
    }
}

/// Plugin runner executable configuration.
#[derive(Debug, Clone, Serialize, Deserialize, Default, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct PluginRunnerConfig {
    pub bin: Option<String>,
}

impl PluginRunnerConfig {
    pub fn merge_from(&mut self, other: Self) {
        if other.bin.is_some() {
            self.bin = other.bin;
        }
    }
}

/// Plugin processor executable configuration.
#[derive(Debug, Clone, Serialize, Deserialize, Default, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct PluginProcessorConfig {
    pub bin: Option<String>,
}

impl PluginProcessorConfig {
    pub fn merge_from(&mut self, other: Self) {
        if other.bin.is_some() {
            self.bin = other.bin;
        }
    }
}
