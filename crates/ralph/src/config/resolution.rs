//! Configuration resolution for Ralph.
//!
//! Responsibilities:
//! - Resolve configuration from multiple layers: global, project, and defaults.
//! - Discover repository root via `.ralph/` directory or `.git/`.
//! - Resolve queue/done file paths and ID generation settings.
//! - Support repo root override via environment variable.
//! - Apply profile patches after base config resolution.
//!
//! Not handled here:
//! - Config file loading/parsing (see `super::layer`).
//! - Config validation (see `super::validation`).
//!
//! Invariants/assumptions:
//! - Config layers are applied in order: defaults, global, project (later overrides earlier).
//! - Paths are resolved relative to repo root unless absolute.
//! - Global config lives at `~/.config/ralph/config.json` (or `$XDG_CONFIG_HOME`).
//! - Project config lives at `.ralph/config.json` relative to repo root.

use crate::constants::defaults::DEFAULT_ID_WIDTH;
use crate::contracts::Config;
use crate::fsutil;
use crate::prompts_internal::util::validate_instruction_file_paths;
use anyhow::{Context, Result, bail};
use std::env;
use std::path::{Path, PathBuf};

use super::Resolved;
use super::layer::{apply_layer, load_layer};
use super::validation::{
    validate_config, validate_queue_done_file_override, validate_queue_file_override,
    validate_queue_id_prefix_override, validate_queue_id_width_override,
};

/// Environment variable for overriding repo root resolution.
pub const REPO_ROOT_OVERRIDE_ENV: &str = "RALPH_REPO_ROOT_OVERRIDE";

/// Resolve configuration from the current working directory.
pub fn resolve_from_cwd() -> Result<Resolved> {
    resolve_from_cwd_internal(true, None)
}

/// Resolve config with an optional profile selection.
///
/// The profile is applied after base config resolution but before instruction_files validation.
pub fn resolve_from_cwd_with_profile(profile: Option<&str>) -> Result<Resolved> {
    resolve_from_cwd_internal(true, profile)
}

/// Resolve config for the doctor command, skipping instruction_files validation.
/// This allows doctor to diagnose and warn about missing files without failing early.
pub fn resolve_from_cwd_for_doctor() -> Result<Resolved> {
    resolve_from_cwd_internal(false, None)
}

fn resolve_from_cwd_internal(
    validate_instruction_files: bool,
    profile: Option<&str>,
) -> Result<Resolved> {
    let cwd = env::current_dir().context("resolve current working directory")?;
    log::debug!("resolving configuration from cwd: {}", cwd.display());
    let repo_root = if let Some(raw_override) = env::var_os(REPO_ROOT_OVERRIDE_ENV) {
        let mut override_path = PathBuf::from(raw_override);
        if override_path.is_relative() {
            override_path = cwd.join(override_path);
        }
        if !override_path.exists() {
            bail!(
                "{} points to missing path: {}",
                REPO_ROOT_OVERRIDE_ENV,
                override_path.display()
            );
        }
        log::debug!(
            "using {} override for repo root: {}",
            REPO_ROOT_OVERRIDE_ENV,
            override_path.display()
        );
        find_repo_root(&override_path)
    } else {
        find_repo_root(&cwd)
    };

    let global_path = global_config_path();
    let project_path = project_config_path(&repo_root);

    let mut cfg = Config::default();

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
        cfg = apply_layer(cfg, layer)
            .with_context(|| format!("apply project config {}", project_path.display()))?;
    }

    validate_config(&cfg)?;

    // Apply selected profile if specified
    if let Some(name) = profile {
        apply_profile_patch(&mut cfg, name)?;
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
/// Config-defined profiles take precedence over built-in profiles.
fn apply_profile_patch(cfg: &mut Config, name: &str) -> Result<()> {
    let name = name.trim();
    if name.is_empty() {
        bail!("Invalid --profile: name cannot be empty");
    }

    let patch =
        crate::agent::resolve_profile_patch(name, cfg.profiles.as_ref()).ok_or_else(|| {
            let names = crate::agent::all_profile_names(cfg.profiles.as_ref());
            anyhow::anyhow!(
                "Unknown profile: {name:?}. Available profiles: {}",
                names.into_iter().collect::<Vec<_>>().join(", ")
            )
        })?;

    cfg.agent.merge_from(patch);
    Ok(())
}

/// Resolve a JSON path with .jsonc fallback.
///
/// Checks if the .json path exists; if not, checks for .jsonc variant.
/// Returns the original path if neither exists (to preserve error messages).
pub fn prefer_json_then_jsonc(json_path: PathBuf) -> PathBuf {
    if json_path.is_file() {
        return json_path;
    }
    let jsonc_path = json_path.with_extension("jsonc");
    if jsonc_path.is_file() {
        return jsonc_path;
    }
    json_path
}

/// Resolve the queue ID prefix from config.
pub fn resolve_id_prefix(cfg: &Config) -> Result<String> {
    validate_queue_id_prefix_override(cfg.queue.id_prefix.as_deref())?;
    let raw = cfg.queue.id_prefix.as_deref().unwrap_or("RQ");
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
        .unwrap_or_else(|| PathBuf::from(".ralph/queue.json"));

    // Check if this is the default path (we'll apply .jsonc fallback to defaults)
    let is_default = raw.as_os_str() == ".ralph/queue.json";

    let value = fsutil::expand_tilde(&raw);
    let resolved = if value.is_absolute() {
        value
    } else {
        repo_root.join(value)
    };

    if is_default {
        // For default path, check .json first, then fall back to .jsonc
        Ok(prefer_json_then_jsonc(resolved))
    } else {
        // For explicit user overrides, use the path as-is
        Ok(resolved)
    }
}

/// Resolve the done file path from config.
pub fn resolve_done_path(repo_root: &Path, cfg: &Config) -> Result<PathBuf> {
    validate_queue_done_file_override(cfg.queue.done_file.as_deref())?;

    // Get the raw path, using default if not specified
    let raw = cfg
        .queue
        .done_file
        .clone()
        .unwrap_or_else(|| PathBuf::from(".ralph/done.json"));

    // Check if this is the default path (we'll apply .jsonc fallback to defaults)
    let is_default = raw.as_os_str() == ".ralph/done.json";

    let value = fsutil::expand_tilde(&raw);
    let resolved = if value.is_absolute() {
        value
    } else {
        repo_root.join(value)
    };

    if is_default {
        // For default path, check .json first, then fall back to .jsonc
        Ok(prefer_json_then_jsonc(resolved))
    } else {
        // For explicit user overrides, use the path as-is
        Ok(resolved)
    }
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
    let json_path = ralph_dir.join("config.json");
    Some(prefer_json_then_jsonc(json_path))
}

/// Get the path to the project config file for a given repo root.
pub fn project_config_path(repo_root: &Path) -> PathBuf {
    let ralph_dir = repo_root.join(".ralph");
    prefer_json_then_jsonc(ralph_dir.join("config.json"))
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
            let has_ralph_marker = ["queue.json", "queue.jsonc", "config.json", "config.jsonc"]
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
