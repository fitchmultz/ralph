//! Configuration resolution for Ralph, including global and project layers.

use crate::contracts::{AgentConfig, Config, ProjectType, QueueConfig};
use crate::fsutil;
use anyhow::{bail, Context, Result};
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
}

pub fn resolve_from_cwd() -> Result<Resolved> {
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
    let layer = serde_json::from_str::<ConfigLayer>(&raw)
        .with_context(|| format!("parse config {} as JSON", path.display()))?;
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
            bail!("Unsupported config version: {}. Ralph requires version 1. Update the 'version' field in your config file.", version);
        }
        base.version = version;
    }

    if let Some(project_type) = layer.project_type {
        base.project_type = Some(project_type);
    }

    base.queue.merge_from(layer.queue);
    base.agent.merge_from(layer.agent);

    Ok(base)
}

pub fn validate_config(cfg: &Config) -> Result<()> {
    if cfg.version != 1 {
        bail!("Unsupported config version: {}. Ralph requires version 1. Update the 'version' field in your config file.", cfg.version);
    }

    if let Some(prefix) = &cfg.queue.id_prefix {
        if prefix.trim().is_empty() {
            bail!("Empty queue.id_prefix: prefix is required if specified. Set a non-empty prefix (e.g., 'RQ') in .ralph/config.json or via --id-prefix.");
        }
    }

    if let Some(width) = cfg.queue.id_width {
        if width == 0 {
            bail!("Invalid queue.id_width: width must be greater than 0. Set a valid width (e.g., 4) in .ralph/config.json or via --id-width.");
        }
    }

    if let Some(file) = &cfg.queue.file {
        if file.as_os_str().is_empty() {
            bail!("Empty queue.file: path is required if specified. Specify a valid path (e.g., '.ralph/queue.json') in .ralph/config.json or via --queue-file.");
        }
    }

    if let Some(done_file) = &cfg.queue.done_file {
        if done_file.as_os_str().is_empty() {
            bail!("Empty queue.done_file: path is required if specified. Specify a valid path (e.g., '.ralph/done.json') in .ralph/config.json or via --done-file.");
        }
    }

    if let Some(phases) = cfg.agent.phases {
        if !(1..=3).contains(&phases) {
            bail!("Invalid agent.phases: {}. Supported values are 1, 2, or 3. Update .ralph/config.json or CLI flags.", phases);
        }
    }

    if let Some(iterations) = cfg.agent.iterations {
        if iterations == 0 {
            bail!("Invalid agent.iterations: {}. Iterations must be greater than 0. Update .ralph/config.json.", iterations);
        }
    }

    if let Some(bin) = &cfg.agent.codex_bin {
        if bin.trim().is_empty() {
            bail!("Empty agent.codex_bin: binary path is required if specified. Set the path to the codex binary in your config.");
        }
    }
    if let Some(bin) = &cfg.agent.opencode_bin {
        if bin.trim().is_empty() {
            bail!("Empty agent.opencode_bin: binary path is required if specified. Set the path to the opencode binary in your config.");
        }
    }
    if let Some(bin) = &cfg.agent.gemini_bin {
        if bin.trim().is_empty() {
            bail!("Empty agent.gemini_bin: binary path is required if specified. Set the path to the gemini binary in your config.");
        }
    }
    if let Some(bin) = &cfg.agent.claude_bin {
        if bin.trim().is_empty() {
            bail!("Empty agent.claude_bin: binary path is required if specified. Set the path to the claude binary in your config.");
        }
    }
    if let Some(bin) = &cfg.agent.cursor_bin {
        if bin.trim().is_empty() {
            bail!("Empty agent.cursor_bin: binary path is required if specified. Set the path to the Cursor agent binary (`agent`) in your config.");
        }
    }

    let ci_gate_enabled = cfg.agent.ci_gate_enabled.unwrap_or(true);
    if ci_gate_enabled {
        if let Some(command) = &cfg.agent.ci_gate_command {
            if command.trim().is_empty() {
                bail!("Empty agent.ci_gate_command: CI gate command must be non-empty when enabled. Set a command (e.g., 'make ci') or disable the gate with agent.ci_gate_enabled=false.");
            }
        }
    }

    Ok(())
}

pub fn resolve_id_prefix(cfg: &Config) -> Result<String> {
    let raw = cfg.queue.id_prefix.as_deref().unwrap_or("RQ");
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        bail!("Empty queue.id_prefix: prefix is required. Set a non-empty prefix (e.g., 'RQ') in .ralph/config.json or via --id-prefix.");
    }
    Ok(trimmed.to_uppercase())
}

pub fn resolve_id_width(cfg: &Config) -> Result<usize> {
    let width = cfg.queue.id_width.unwrap_or(4) as usize;
    if width == 0 {
        bail!("Invalid_queue.id_width: width must be greater than 0. Set a valid width (e.g., 4) in .ralph/config.json or via --id-width.");
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
        bail!("Empty queue.file: path is required. Specify a valid path (e.g., '.ralph/queue.json') in .ralph/config.json or via --queue-file.");
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
        bail!("Empty queue.done_file: path is required. Specify a valid path (e.g., '.ralph/done.json') in .ralph/config.json or via --done-file.");
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
    fn validate_config_rejects_empty_cursor_bin() {
        let mut cfg = Config::default();
        cfg.agent.cursor_bin = Some("   ".to_string());

        let err = validate_config(&cfg).expect_err("expected validation to fail");
        assert!(err.to_string().contains("agent.cursor_bin"));
    }
}
