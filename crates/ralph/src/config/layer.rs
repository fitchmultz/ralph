//! Configuration layer handling for Ralph.
//!
//! Responsibilities:
//! - Define `ConfigLayer` for partial config from JSON files.
//! - Load config layers with JSONC comment support.
//! - Save config layers with automatic directory creation.
//! - Apply/merge layers into base configuration.
//!
//! Not handled here:
//! - Config validation (see `super::validation`).
//! - Path resolution (see `super::resolution`).
//! - Profile application (see `super::resolution`).
//!
//! Invariants/assumptions:
//! - `save_layer` creates parent directories automatically if needed.
//! - `apply_layer` merges using leaf-wise semantics for nested structures.
//! - Version must be 1; unsupported versions are rejected during apply.

use crate::contracts::{
    AgentConfig, Config, LoopConfig, ParallelConfig, PluginsConfig, ProjectType, QueueConfig,
};
use crate::fsutil;
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ConfigLayer {
    pub version: Option<u32>,
    pub project_type: Option<ProjectType>,
    pub queue: QueueConfig,
    pub agent: AgentConfig,
    pub parallel: ParallelConfig,
    #[serde(rename = "loop")]
    pub loop_field: LoopConfig,
    pub plugins: PluginsConfig,
    /// Named profiles for quick workflow switching.
    pub profiles: Option<BTreeMap<String, AgentConfig>>,
}

/// Load a config layer from a JSON/JSONC file.
pub fn load_layer(path: &Path) -> Result<ConfigLayer> {
    let raw = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    let layer =
        crate::jsonc::parse_jsonc::<ConfigLayer>(&raw, &format!("config {}", path.display()))?;
    Ok(layer)
}

/// Save a config layer to a JSON file.
/// Automatically sets version to 1 if not specified and creates parent directories.
pub fn save_layer(path: &Path, layer: &ConfigLayer) -> Result<()> {
    let mut to_save = layer.clone();
    if to_save.version.is_none() {
        to_save.version = Some(1);
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("create config directory {}", parent.display()))?;
    }
    let rendered = serde_json::to_string_pretty(&to_save).context("serialize config JSON")?;
    fsutil::write_atomic(path, rendered.as_bytes())
        .with_context(|| format!("write config JSON {}", path.display()))?;
    Ok(())
}

/// Apply a config layer on top of a base config.
/// Later layers override earlier ones using leaf-wise merge semantics.
pub fn apply_layer(mut base: Config, layer: ConfigLayer) -> Result<Config> {
    if let Some(version) = layer.version {
        if version != 1 {
            bail!(
                "Unsupported config version: {}. Ralph requires version 1. Update the 'version' field in your config file.",
                version
            );
        }
        base.version = version;
    }

    if let Some(project_type) = layer.project_type {
        base.project_type = Some(project_type);
    }

    base.queue.merge_from(layer.queue);
    base.agent.merge_from(layer.agent);
    base.parallel.merge_from(layer.parallel);
    base.loop_field.merge_from(layer.loop_field);
    base.plugins.merge_from(layer.plugins);

    // Merge profiles across layers
    if let Some(profiles) = layer.profiles {
        let base_profiles = base.profiles.get_or_insert_with(BTreeMap::new);
        for (name, patch) in profiles {
            base_profiles
                .entry(name)
                .and_modify(|existing| existing.merge_from(patch.clone()))
                .or_insert(patch);
        }
    }

    Ok(base)
}
