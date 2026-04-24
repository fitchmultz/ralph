//! Configuration resolution for Ralph.
//!
//! Purpose:
//! - Configuration resolution for Ralph.
//!
//! Responsibilities:
//! - Resolve configuration from multiple layers: global, project, and defaults.
//! - Discover repository root via `.ralph/` directory or `.git/`.
//! - Resolve queue/done file paths and ID generation settings.
//! - Apply profile patches after base config resolution.
//!
//! Not handled here:
//! - Config file loading/parsing (see `super::layer`).
//! - Config validation (see `super::validation`).
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Config layers are applied in order: defaults, global, project (later overrides earlier).
//! - Paths are resolved relative to repo root unless absolute.
//! - Global config resolves from `~/.config/ralph/config.jsonc`.
//! - Project config resolves from `.ralph/config.jsonc`.

use crate::constants::defaults::DEFAULT_ID_WIDTH;
use crate::constants::queue::{DEFAULT_DONE_FILE, DEFAULT_ID_PREFIX, DEFAULT_QUEUE_FILE};
use crate::contracts::Config;
use crate::fsutil;
use crate::prompts_internal::validate_instruction_file_paths;
use anyhow::{Context, Result, bail};
use std::env;
use std::path::{Path, PathBuf};

use super::Resolved;
use super::layer::{ConfigLayer, apply_layer, load_layer};
use super::trust::load_repo_trust;
use super::validation::{
    validate_config, validate_project_execution_trust, validate_queue_done_file_override,
    validate_queue_file_override, validate_queue_id_prefix_override,
    validate_queue_id_width_override,
};

/// Resolve configuration from the current working directory.
pub fn resolve_from_cwd() -> Result<Resolved> {
    resolve_from_cwd_internal(true, true, None)
}

/// Resolve like `resolve_from_cwd`, but skip project-layer execution trust validation.
///
/// Used when the operator is explicitly opting into trust (for example `ralph init
/// --trust-project-commands`) so initialization can proceed before `.ralph/trust.jsonc`
/// exists, then the trust file is written afterward.
pub fn resolve_from_cwd_skipping_project_execution_trust() -> Result<Resolved> {
    resolve_from_cwd_internal(true, false, None)
}

/// Resolve config with an optional profile selection.
///
/// The profile is applied after base config resolution but before instruction_files validation.
pub fn resolve_from_cwd_with_profile(profile: Option<&str>) -> Result<Resolved> {
    resolve_from_cwd_internal(true, true, profile)
}

/// Resolve config for the doctor command, skipping instruction_files validation.
/// This allows doctor to diagnose and warn about missing files without failing early.
pub fn resolve_from_cwd_for_doctor() -> Result<Resolved> {
    resolve_from_cwd_internal(false, false, None)
}

fn resolve_from_cwd_internal(
    validate_instruction_files: bool,
    validate_execution_trust: bool,
    profile: Option<&str>,
) -> Result<Resolved> {
    let cwd = env::current_dir().context("resolve current working directory")?;
    log::debug!("resolving configuration from cwd: {}", cwd.display());
    let repo_root = find_repo_root(&cwd);

    let global_path = global_config_path();
    let project_path = project_config_path(&repo_root);
    let repo_trust = load_repo_trust(&repo_root)?;

    let mut cfg = Config::default();
    let mut project_layer: Option<ConfigLayer> = None;

    if let Some(path) = global_path.as_ref() {
        log::debug!("checking global config at: {}", path.display());
        if path.exists() {
            log::debug!("loading global config: {}", path.display());
            let layer = load_layer(path)
                .with_context(|| format!("load global config {}", path.display()))?;
            cfg = apply_layer(cfg, layer)
                .with_context(|| format!("apply global config {}", path.display()))?;
        }
    }

    log::debug!("checking project config at: {}", project_path.display());
    if project_path.exists() {
        log::debug!("loading project config: {}", project_path.display());
        let layer = load_layer(&project_path)
            .with_context(|| format!("load project config {}", project_path.display()))?;
        project_layer = Some(layer.clone());
        cfg = apply_layer(cfg, layer)
            .with_context(|| format!("apply project config {}", project_path.display()))?;
    }

    if validate_execution_trust {
        validate_project_execution_trust(project_layer.as_ref(), &repo_trust)?;
    }
    validate_config(&cfg)?;

    // Apply selected profile if specified
    if let Some(name) = profile {
        apply_profile_patch(&mut cfg, name)?;
        validate_config(&cfg)?;
    }

    // Validate instruction_files early for fast feedback (before runtime prompt rendering)
    if validate_instruction_files {
        validate_instruction_file_paths(&repo_root, &cfg)
            .with_context(|| "validate instruction_files from config")?;
    }

    let id_prefix = resolve_id_prefix(&cfg)?;
    let id_width = resolve_id_width(&cfg)?;
    let queue_path = resolve_queue_path(&repo_root, &cfg)?;
    let done_path = resolve_done_path(&repo_root, &cfg)?;

    log::debug!("resolved repo_root: {}", repo_root.display());
    log::debug!("resolved queue_path: {}", queue_path.display());
    log::debug!("resolved done_path: {}", done_path.display());

    Ok(Resolved {
        config: cfg,
        repo_root,
        queue_path,
        done_path,
        id_prefix,
        id_width,
        global_config_path: global_path,
        project_config_path: Some(project_path),
    })
}

/// Apply a named profile patch to the resolved config.
///
/// Profile values are merged into `cfg.agent` using leaf-wise merge semantics.
fn apply_profile_patch(cfg: &mut Config, name: &str) -> Result<()> {
    let name = name.trim();
    if name.is_empty() {
        bail!("Invalid --profile: name cannot be empty");
    }

    let patch =
        crate::agent::resolve_profile_patch(name, cfg.profiles.as_ref()).ok_or_else(|| {
            let names = crate::agent::all_profile_names(cfg.profiles.as_ref());
            if names.is_empty() {
                anyhow::anyhow!(
                    "Unknown profile: {name:?}. No profiles are configured. Define profiles under the `profiles` key in .ralph/config.jsonc or ~/.config/ralph/config.jsonc."
                )
            } else {
                anyhow::anyhow!(
                    "Unknown profile: {name:?}. Available configured profiles: {}",
                    names.into_iter().collect::<Vec<_>>().join(", ")
                )
            }
        })?;

    cfg.agent.merge_from(patch);
    Ok(())
}

/// Resolve the queue ID prefix from config.
pub fn resolve_id_prefix(cfg: &Config) -> Result<String> {
    validate_queue_id_prefix_override(cfg.queue.id_prefix.as_deref())?;
    let raw = cfg.queue.id_prefix.as_deref().unwrap_or(DEFAULT_ID_PREFIX);
    Ok(raw.trim().to_uppercase())
}

/// Resolve the queue ID width from config.
pub fn resolve_id_width(cfg: &Config) -> Result<usize> {
    validate_queue_id_width_override(cfg.queue.id_width)?;
    Ok(cfg.queue.id_width.unwrap_or(DEFAULT_ID_WIDTH as u8) as usize)
}

/// Resolve the queue file path from config.
pub fn resolve_queue_path(repo_root: &Path, cfg: &Config) -> Result<PathBuf> {
    validate_queue_file_override(cfg.queue.file.as_deref())?;

    // Get the raw path, using default if not specified
    let raw = cfg
        .queue
        .file
        .clone()
        .unwrap_or_else(|| PathBuf::from(DEFAULT_QUEUE_FILE));

    let value = fsutil::expand_tilde(&raw);
    Ok(if value.is_absolute() {
        value
    } else {
        repo_root.join(value)
    })
}

/// Resolve the done file path from config.
pub fn resolve_done_path(repo_root: &Path, cfg: &Config) -> Result<PathBuf> {
    validate_queue_done_file_override(cfg.queue.done_file.as_deref())?;

    // Get the raw path, using default if not specified
    let raw = cfg
        .queue
        .done_file
        .clone()
        .unwrap_or_else(|| PathBuf::from(DEFAULT_DONE_FILE));

    let value = fsutil::expand_tilde(&raw);
    Ok(if value.is_absolute() {
        value
    } else {
        repo_root.join(value)
    })
}

/// Get the path to the global config file.
pub fn global_config_path() -> Option<PathBuf> {
    let base = if let Some(value) = env::var_os("XDG_CONFIG_HOME") {
        PathBuf::from(value)
    } else {
        let home = env::var_os("HOME")?;
        PathBuf::from(home).join(".config")
    };
    let ralph_dir = base.join("ralph");
    Some(ralph_dir.join("config.jsonc"))
}

/// Get the path to the project config file for a given repo root.
pub fn project_config_path(repo_root: &Path) -> PathBuf {
    let ralph_dir = repo_root.join(".ralph");
    ralph_dir.join("config.jsonc")
}

/// Find the repository root starting from a given path.
///
/// Searches upward for a `.ralph/` directory with marker files
/// or a `.git/` directory.
pub fn find_repo_root(start: &Path) -> PathBuf {
    log::debug!("searching for repo root starting from: {}", start.display());
    for dir in start.ancestors() {
        log::debug!("checking directory: {}", dir.display());
        let ralph_dir = dir.join(".ralph");
        if ralph_dir.is_dir() {
            let has_ralph_marker = ["queue.jsonc", "config.jsonc", "done.jsonc"]
                .iter()
                .any(|name| ralph_dir.join(name).is_file());
            if has_ralph_marker {
                log::debug!("found repo root at: {} (via .ralph/)", dir.display());
                return dir.to_path_buf();
            }
        }
        if dir.join(".git").exists() {
            log::debug!("found repo root at: {} (via .git/)", dir.display());
            return dir.to_path_buf();
        }
    }
    log::debug!(
        "no repo root found, using start directory: {}",
        start.display()
    );
    start.to_path_buf()
}
