use crate::contracts::{AgentConfig, Config, ProjectType, QueueConfig};
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
        bail!("Invalid queue.id_width: width must be greater than 0. Set a valid width (e.g., 4) in .ralph/config.json or via --id-width.");
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
