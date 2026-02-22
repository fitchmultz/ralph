//! Configuration resolution for Ralph, including global and project layers.
//!
//! Responsibilities:
//! - Resolve configuration from multiple layers: global config, project config, and defaults.
//! - Load and parse config files (JSON with JSONC comment support via `load_layer`).
//! - Merge configuration layers via `ConfigLayer` and `apply_layer`.
//! - Validate configuration values (version, paths, numeric ranges, runner binaries).
//! - Resolve queue/done file paths and ID generation settings (prefix, width).
//! - Discover repository root via `.ralph/` directory or `.git/`.
//!
//! Not handled here:
//! - CLI argument parsing (see `crate::cli`).
//! - Queue operations like task CRUD (see `crate::queue`).
//! - Runner execution or agent invocation (see `crate::runner`).
//! - Prompt rendering or template processing (see `crate::prompts_internal`).
//! - Lock management (see `crate::lock`).
//!
//! Invariants/assumptions:
//! - Config version must be 1; unsupported versions are rejected.
//! - Paths are resolved relative to repo root unless absolute.
//! - Global config resolves from `~/.config/ralph/config.jsonc` with `.json` fallback.
//! - Project config resolves from `.ralph/config.jsonc` with `.json` fallback.
//! - Config layers are applied in this order: defaults, then global, then project (later layers override earlier ones).
//! - `save_layer` creates parent directories automatically if needed.

use std::path::PathBuf;

mod layer;
mod resolution;
mod validation;

#[cfg(test)]
mod tests;

// Re-export main types and functions for backward compatibility
pub use layer::{ConfigLayer, apply_layer, load_layer, save_layer};
pub use resolution::{
    find_repo_root, global_config_path, prefer_jsonc_then_json, project_config_path,
    resolve_done_path, resolve_from_cwd, resolve_from_cwd_for_doctor,
    resolve_from_cwd_with_profile, resolve_id_prefix, resolve_id_width, resolve_queue_path,
};
pub use validation::{
    git_ref_invalid_reason, validate_agent_binary_paths, validate_agent_patch, validate_config,
    validate_queue_done_file_override, validate_queue_file_override,
    validate_queue_id_prefix_override, validate_queue_id_width_override, validate_queue_overrides,
};

/// Resolved configuration including computed paths.
#[derive(Debug, Clone)]
pub struct Resolved {
    pub config: crate::contracts::Config,
    pub repo_root: PathBuf,
    pub queue_path: PathBuf,
    pub done_path: PathBuf,
    pub id_prefix: String,
    pub id_width: usize,
    pub global_config_path: Option<PathBuf>,
    pub project_config_path: Option<PathBuf>,
}
