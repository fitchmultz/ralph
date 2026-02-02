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
//! - Global config lives at `~/.config/ralph/config.json` (or `$XDG_CONFIG_HOME/ralph/config.json`).
//! - Project config lives at `.ralph/config.json` relative to repo root.
//! - Config merging follows precedence: global → project → defaults.
//! - `save_layer` creates parent directories automatically if needed.

use crate::constants::defaults::DEFAULT_ID_WIDTH;
use crate::contracts::{AgentConfig, Config, ParallelConfig, ProjectType, QueueConfig, TuiConfig};
use crate::fsutil;
use crate::prompts_internal::util::validate_instruction_file_paths;
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct Resolved {
    pub config: Config,
    pub repo_root: PathBuf,
    pub queue_path: PathBuf,
    pub done_path: PathBuf,
    pub id_prefix: String,
    pub id_width: usize,
    pub global_config_path: Option<PathBuf>,
    pub project_config_path: Option<PathBuf>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ConfigLayer {
    pub version: Option<u32>,
    pub project_type: Option<ProjectType>,
    pub queue: QueueConfig,
    pub agent: AgentConfig,
    pub parallel: ParallelConfig,
    pub tui: TuiConfig,
}

pub fn resolve_from_cwd() -> Result<Resolved> {
    resolve_from_cwd_internal(true)
}

/// Resolve config for the doctor command, skipping instruction_files validation.
/// This allows doctor to diagnose and warn about missing files without failing early.
pub fn resolve_from_cwd_for_doctor() -> Result<Resolved> {
    resolve_from_cwd_internal(false)
}

fn resolve_from_cwd_internal(validate_instruction_files: bool) -> Result<Resolved> {
    let cwd = env::current_dir().context("resolve current working directory")?;
    log::debug!("resolving configuration from cwd: {}", cwd.display());
    let repo_root = find_repo_root(&cwd);

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

pub fn load_layer(path: &Path) -> Result<ConfigLayer> {
    let raw = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    let layer =
        crate::jsonc::parse_jsonc::<ConfigLayer>(&raw, &format!("config {}", path.display()))?;
    Ok(layer)
}

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
    base.tui.merge_from(layer.tui);

    Ok(base)
}

pub fn validate_config(cfg: &Config) -> Result<()> {
    if cfg.version != 1 {
        bail!(
            "Unsupported config version: {}. Ralph requires version 1. Update the 'version' field in your config file.",
            cfg.version
        );
    }

    if let Some(prefix) = &cfg.queue.id_prefix
        && prefix.trim().is_empty()
    {
        bail!(
            "Empty queue.id_prefix: prefix is required if specified. Set a non-empty prefix (e.g., 'RQ') in .ralph/config.json or via --id-prefix."
        );
    }

    if let Some(width) = cfg.queue.id_width
        && width == 0
    {
        bail!(
            "Invalid queue.id_width: width must be greater than 0. Set a valid width (e.g., 4) in .ralph/config.json or via --id-width."
        );
    }

    if let Some(file) = &cfg.queue.file
        && file.as_os_str().is_empty()
    {
        bail!(
            "Empty queue.file: path is required if specified. Specify a valid path (e.g., '.ralph/queue.json') in .ralph/config.json or via --queue-file."
        );
    }

    if let Some(done_file) = &cfg.queue.done_file
        && done_file.as_os_str().is_empty()
    {
        bail!(
            "Empty queue.done_file: path is required if specified. Specify a valid path (e.g., '.ralph/done.json') in .ralph/config.json or via --done-file."
        );
    }

    if let Some(phases) = cfg.agent.phases
        && !(1..=3).contains(&phases)
    {
        bail!(
            "Invalid agent.phases: {}. Supported values are 1, 2, or 3. Update .ralph/config.json or CLI flags.",
            phases
        );
    }

    if let Some(iterations) = cfg.agent.iterations
        && iterations == 0
    {
        bail!(
            "Invalid agent.iterations: {}. Iterations must be greater than 0. Update .ralph/config.json.",
            iterations
        );
    }

    if let Some(workers) = cfg.parallel.workers
        && workers < 2
    {
        bail!(
            "Invalid parallel.workers: {}. Parallel workers must be >= 2. Update .ralph/config.json or CLI flags.",
            workers
        );
    }

    if let Some(retries) = cfg.parallel.merge_retries
        && retries == 0
    {
        bail!(
            "Invalid parallel.merge_retries: {}. merge_retries must be >= 1. Update .ralph/config.json.",
            retries
        );
    }

    if let Some(prefix) = &cfg.parallel.branch_prefix
        && prefix.trim().is_empty()
    {
        bail!(
            "Invalid parallel.branch_prefix: prefix must be non-empty. Update .ralph/config.json."
        );
    }

    if let Some(timeout) = cfg.agent.session_timeout_hours
        && timeout == 0
    {
        bail!(
            "Invalid agent.session_timeout_hours: {}. Session timeout must be greater than 0. Update .ralph/config.json.",
            timeout
        );
    }

    if let Some(bin) = &cfg.agent.codex_bin
        && bin.trim().is_empty()
    {
        bail!(
            "Empty agent.codex_bin: binary path is required if specified. Set the path to the codex binary in your config."
        );
    }
    if let Some(bin) = &cfg.agent.opencode_bin
        && bin.trim().is_empty()
    {
        bail!(
            "Empty agent.opencode_bin: binary path is required if specified. Set the path to the opencode binary in your config."
        );
    }
    if let Some(bin) = &cfg.agent.gemini_bin
        && bin.trim().is_empty()
    {
        bail!(
            "Empty agent.gemini_bin: binary path is required if specified. Set the path to the gemini binary in your config."
        );
    }
    if let Some(bin) = &cfg.agent.claude_bin
        && bin.trim().is_empty()
    {
        bail!(
            "Empty agent.claude_bin: binary path is required if specified. Set the path to the claude binary in your config."
        );
    }
    if let Some(bin) = &cfg.agent.cursor_bin
        && bin.trim().is_empty()
    {
        bail!(
            "Empty agent.cursor_bin: binary path is required if specified. Set the path to the Cursor agent binary (`agent`) in your config."
        );
    }

    let ci_gate_enabled = cfg.agent.ci_gate_enabled.unwrap_or(true);
    if ci_gate_enabled
        && let Some(command) = &cfg.agent.ci_gate_command
        && command.trim().is_empty()
    {
        bail!(
            "Empty agent.ci_gate_command: CI gate command must be non-empty when enabled. Set a command (e.g., 'make ci') or disable the gate with agent.ci_gate_enabled=false."
        );
    }

    Ok(())
}

pub fn resolve_id_prefix(cfg: &Config) -> Result<String> {
    let raw = cfg.queue.id_prefix.as_deref().unwrap_or("RQ");
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        bail!(
            "Empty queue.id_prefix: prefix is required. Set a non-empty prefix (e.g., 'RQ') in .ralph/config.json or via --id-prefix."
        );
    }
    Ok(trimmed.to_uppercase())
}

pub fn resolve_id_width(cfg: &Config) -> Result<usize> {
    let width = cfg.queue.id_width.unwrap_or(DEFAULT_ID_WIDTH as u8) as usize;
    if width == 0 {
        bail!(
            "Invalid_queue.id_width: width must be greater than 0. Set a valid width (e.g., 4) in .ralph/config.json or via --id-width."
        );
    }
    Ok(width)
}

pub fn resolve_queue_path(repo_root: &Path, cfg: &Config) -> Result<PathBuf> {
    let value = cfg
        .queue
        .file
        .clone()
        .unwrap_or_else(|| PathBuf::from(".ralph/queue.json"));
    if value.as_os_str().is_empty() {
        bail!(
            "Empty queue.file: path is required. Specify a valid path (e.g., '.ralph/queue.json') in .ralph/config.json or via --queue-file."
        );
    }
    if value.is_absolute() {
        return Ok(value);
    }
    Ok(repo_root.join(value))
}

pub fn resolve_done_path(repo_root: &Path, cfg: &Config) -> Result<PathBuf> {
    let value = cfg
        .queue
        .done_file
        .clone()
        .unwrap_or_else(|| PathBuf::from(".ralph/done.json"));
    if value.as_os_str().is_empty() {
        bail!(
            "Empty queue.done_file: path is required. Specify a valid path (e.g., '.ralph/done.json') in .ralph/config.json or via --done-file."
        );
    }
    if value.is_absolute() {
        return Ok(value);
    }
    Ok(repo_root.join(value))
}

pub fn global_config_path() -> Option<PathBuf> {
    let base = if let Some(value) = env::var_os("XDG_CONFIG_HOME") {
        PathBuf::from(value)
    } else {
        let home = env::var_os("HOME")?;
        PathBuf::from(home).join(".config")
    };
    let ralph_dir = base.join("ralph");
    let json_path = ralph_dir.join("config.json");
    Some(json_path)
}

pub fn project_config_path(repo_root: &Path) -> PathBuf {
    let ralph_dir = repo_root.join(".ralph");
    ralph_dir.join("config.json")
}

pub fn find_repo_root(start: &Path) -> PathBuf {
    log::debug!("searching for repo root starting from: {}", start.display());
    for dir in start.ancestors() {
        log::debug!("checking directory: {}", dir.display());
        let ralph_dir = dir.join(".ralph");
        if ralph_dir.is_dir() {
            let has_json =
                ralph_dir.join("queue.json").is_file() || ralph_dir.join("config.json").is_file();
            if has_json {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contracts::GitRevertMode;

    #[test]
    fn apply_layer_overrides_git_revert_mode() -> Result<()> {
        let base = Config::default();
        let mut layer = ConfigLayer::default();
        layer.agent.git_revert_mode = Some(GitRevertMode::Disabled);

        let merged = apply_layer(base, layer)?;
        assert_eq!(
            merged.agent.git_revert_mode.unwrap_or(GitRevertMode::Ask),
            GitRevertMode::Disabled
        );
        Ok(())
    }

    #[test]
    fn apply_layer_overrides_git_commit_push_enabled() -> Result<()> {
        let base = Config::default();
        let mut layer = ConfigLayer::default();
        layer.agent.git_commit_push_enabled = Some(false);

        let merged = apply_layer(base, layer)?;
        assert_eq!(merged.agent.git_commit_push_enabled, Some(false));
        Ok(())
    }

    #[test]
    fn save_layer_writes_version_and_round_trips() -> Result<()> {
        let temp = tempfile::TempDir::new()?;
        let path = temp.path().join("config.json");
        let layer = ConfigLayer::default();

        save_layer(&path, &layer)?;
        let loaded = load_layer(&path)?;

        assert_eq!(loaded.version, Some(1));
        Ok(())
    }

    #[test]
    fn validate_config_rejects_empty_ci_gate_command_when_enabled() {
        let mut cfg = Config::default();
        cfg.agent.ci_gate_command = Some("   ".to_string());
        cfg.agent.ci_gate_enabled = Some(true);

        let err = validate_config(&cfg).expect_err("expected validation to fail");
        assert!(err.to_string().contains("agent.ci_gate_command"));
    }

    #[test]
    fn validate_config_allows_empty_ci_gate_command_when_disabled() {
        let mut cfg = Config::default();
        cfg.agent.ci_gate_command = Some(" ".to_string());
        cfg.agent.ci_gate_enabled = Some(false);

        validate_config(&cfg).expect("validation should pass when disabled");
    }

    #[test]
    fn validate_config_rejects_zero_iterations() {
        let mut cfg = Config::default();
        cfg.agent.iterations = Some(0);

        let err = validate_config(&cfg).expect_err("expected validation to fail");
        assert!(err.to_string().contains("agent.iterations"));
    }

    #[test]
    fn validate_config_rejects_parallel_workers_lt_two() {
        let mut cfg = Config::default();
        cfg.parallel.workers = Some(1);

        let err = validate_config(&cfg).expect_err("expected validation to fail");
        assert!(err.to_string().contains("parallel.workers"));
    }

    #[test]
    fn validate_config_rejects_parallel_merge_retries_zero() {
        let mut cfg = Config::default();
        cfg.parallel.merge_retries = Some(0);

        let err = validate_config(&cfg).expect_err("expected validation to fail");
        assert!(err.to_string().contains("parallel.merge_retries"));
    }

    #[test]
    fn validate_config_rejects_parallel_branch_prefix_empty() {
        let mut cfg = Config::default();
        cfg.parallel.branch_prefix = Some("   ".to_string());

        let err = validate_config(&cfg).expect_err("expected validation to fail");
        assert!(err.to_string().contains("parallel.branch_prefix"));
    }

    #[test]
    fn validate_config_rejects_zero_session_timeout_hours() {
        let mut cfg = Config::default();
        cfg.agent.session_timeout_hours = Some(0);

        let err = validate_config(&cfg).expect_err("expected validation to fail");
        assert!(err.to_string().contains("agent.session_timeout_hours"));
    }

    #[test]
    fn validate_config_rejects_empty_cursor_bin() {
        let mut cfg = Config::default();
        cfg.agent.cursor_bin = Some("   ".to_string());

        let err = validate_config(&cfg).expect_err("expected validation to fail");
        assert!(err.to_string().contains("agent.cursor_bin"));
    }

    // Tests for instruction_files validation (validate_instruction_file_paths)

    #[test]
    fn validate_instruction_file_paths_rejects_missing_file() {
        let temp = tempfile::TempDir::new().unwrap();
        let mut cfg = Config::default();
        cfg.agent.instruction_files = Some(vec![PathBuf::from("nonexistent.md")]);

        let err = validate_instruction_file_paths(temp.path(), &cfg).expect_err("should fail");
        let msg = err.to_string();
        assert!(
            msg.contains("nonexistent.md"),
            "Error should mention the file: {}",
            msg
        );
        assert!(
            msg.contains("read bytes from") || msg.contains("No such file"),
            "Error should indicate file not found: {}",
            msg
        );
    }

    #[test]
    fn validate_instruction_file_paths_accepts_valid_file() {
        let temp = tempfile::TempDir::new().unwrap();
        let file_path = temp.path().join("valid.md");
        std::fs::write(&file_path, "Valid instruction content").unwrap();

        let mut cfg = Config::default();
        cfg.agent.instruction_files = Some(vec![file_path]);

        validate_instruction_file_paths(temp.path(), &cfg).expect("should pass");
    }

    #[test]
    fn validate_instruction_file_paths_rejects_empty_file() {
        let temp = tempfile::TempDir::new().unwrap();
        let file_path = temp.path().join("empty.md");
        std::fs::write(&file_path, "").unwrap();

        let mut cfg = Config::default();
        cfg.agent.instruction_files = Some(vec![file_path]);

        let err = validate_instruction_file_paths(temp.path(), &cfg).expect_err("should fail");
        assert!(
            err.to_string().contains("empty"),
            "Error should indicate file is empty"
        );
    }

    #[test]
    fn validate_instruction_file_paths_rejects_non_utf8_file() {
        let temp = tempfile::TempDir::new().unwrap();
        let file_path = temp.path().join("invalid.md");
        // Write invalid UTF-8 bytes
        std::fs::write(&file_path, vec![0x80, 0x81, 0x82]).unwrap();

        let mut cfg = Config::default();
        cfg.agent.instruction_files = Some(vec![file_path]);

        let err = validate_instruction_file_paths(temp.path(), &cfg).expect_err("should fail");
        assert!(
            err.to_string().contains("UTF-8"),
            "Error should indicate invalid UTF-8: {}",
            err
        );
    }

    #[test]
    fn validate_instruction_file_paths_resolves_relative_paths() {
        let temp = tempfile::TempDir::new().unwrap();
        let file_path = temp.path().join("instructions.md");
        std::fs::write(&file_path, "Content").unwrap();

        let mut cfg = Config::default();
        // Use relative path
        cfg.agent.instruction_files = Some(vec![PathBuf::from("instructions.md")]);

        validate_instruction_file_paths(temp.path(), &cfg).expect("should pass");
    }

    #[test]
    fn validate_instruction_file_paths_resolves_absolute_paths() {
        let temp = tempfile::TempDir::new().unwrap();
        let file_path = temp.path().join("absolute.md");
        std::fs::write(&file_path, "Absolute path content").unwrap();

        let mut cfg = Config::default();
        // Use absolute path
        cfg.agent.instruction_files = Some(vec![file_path.clone()]);

        validate_instruction_file_paths(temp.path(), &cfg).expect("should pass");
    }

    #[test]
    fn validate_instruction_file_paths_is_noop_when_none_configured() {
        let temp = tempfile::TempDir::new().unwrap();
        let cfg = Config::default();

        // Should not fail when instruction_files is None
        validate_instruction_file_paths(temp.path(), &cfg).expect("should pass with no files");
    }

    #[test]
    fn validate_instruction_file_paths_validates_all_files_and_fails_on_first_error() {
        let temp = tempfile::TempDir::new().unwrap();

        // Create one valid file and one missing file
        let valid_path = temp.path().join("valid.md");
        std::fs::write(&valid_path, "Valid content").unwrap();

        let mut cfg = Config::default();
        cfg.agent.instruction_files = Some(vec![PathBuf::from("missing.md"), valid_path]);

        let err = validate_instruction_file_paths(temp.path(), &cfg).expect_err("should fail");
        assert!(
            err.to_string().contains("missing.md"),
            "Error should mention the first missing file"
        );
    }
}
