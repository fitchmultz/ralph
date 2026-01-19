use crate::contracts::{AgentConfig, Config, ProjectType, QueueConfig};
use anyhow::{anyhow, bail, Context, Result};
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
struct ConfigLayer {
    pub version: Option<u32>,
    pub project_type: Option<ProjectType>,
    pub queue: QueueConfig,
    pub agent: AgentConfig,
}

pub fn resolve_from_cwd() -> Result<Resolved> {
    let cwd = env::current_dir().context("resolve current working directory")?;
    let repo_root = find_repo_root(&cwd);

    let global_path = global_config_path();
    let project_path = project_config_path(&repo_root);

    let mut cfg = Config::default();

    if let Some(path) = global_path.as_ref() {
        if path.exists() {
            let layer = load_layer(path)
                .with_context(|| format!("load global config {}", path.display()))?;
            cfg = apply_layer(cfg, layer)?;
        }
    }

    if project_path.exists() {
        let layer = load_layer(&project_path)
            .with_context(|| format!("load project config {}", project_path.display()))?;
        cfg = apply_layer(cfg, layer)?;
    }

    if cfg.version != 1 {
        return Err(anyhow!("config version must be 1 (got {})", cfg.version));
    }

    let id_prefix = resolve_id_prefix(&cfg)?;
    let id_width = resolve_id_width(&cfg)?;
    let queue_path = resolve_queue_path(&repo_root, &cfg)?;
    let done_path = resolve_done_path(&repo_root, &cfg)?;

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

fn load_layer(path: &Path) -> Result<ConfigLayer> {
    let raw = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    let layer: ConfigLayer =
        serde_yaml::from_str(&raw).with_context(|| format!("parse YAML {}", path.display()))?;
    Ok(layer)
}

fn apply_layer(mut base: Config, layer: ConfigLayer) -> Result<Config> {
    if let Some(version) = layer.version {
        if version != 1 {
            bail!("config version must be 1 (got {})", version);
        }
        base.version = version;
    }

    if let Some(project_type) = layer.project_type {
        base.project_type = Some(project_type);
    }

    if let Some(file) = layer.queue.file {
        base.queue.file = Some(file);
    }
    if let Some(file) = layer.queue.done_file {
        base.queue.done_file = Some(file);
    }
    if let Some(prefix) = layer.queue.id_prefix {
        base.queue.id_prefix = Some(prefix);
    }
    if let Some(width) = layer.queue.id_width {
        base.queue.id_width = Some(width);
    }

    if let Some(runner) = layer.agent.runner {
        base.agent.runner = Some(runner);
    }
    if let Some(model) = layer.agent.model {
        base.agent.model = Some(model);
    }
    if let Some(effort) = layer.agent.reasoning_effort {
        base.agent.reasoning_effort = Some(effort);
    }
    if let Some(bin) = layer.agent.codex_bin {
        base.agent.codex_bin = Some(bin);
    }
    if let Some(bin) = layer.agent.opencode_bin {
        base.agent.opencode_bin = Some(bin);
    }
    if let Some(bin) = layer.agent.gemini_bin {
        base.agent.gemini_bin = Some(bin);
    }

    Ok(base)
}

fn resolve_id_prefix(cfg: &Config) -> Result<String> {
    let raw = cfg.queue.id_prefix.as_deref().unwrap_or("RQ");
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        bail!("queue.id_prefix must be non-empty");
    }
    Ok(trimmed.to_uppercase())
}

fn resolve_id_width(cfg: &Config) -> Result<usize> {
    let width = cfg.queue.id_width.unwrap_or(4) as usize;
    if width == 0 {
        bail!("queue.id_width must be > 0");
    }
    Ok(width)
}

fn resolve_queue_path(repo_root: &Path, cfg: &Config) -> Result<PathBuf> {
    let value = cfg
        .queue
        .file
        .clone()
        .unwrap_or_else(|| PathBuf::from(".ralph/queue.yaml"));
    if value.as_os_str().is_empty() {
        bail!("queue.file must be non-empty when set");
    }
    if value.is_absolute() {
        return Ok(value);
    }
    Ok(repo_root.join(value))
}

fn resolve_done_path(repo_root: &Path, cfg: &Config) -> Result<PathBuf> {
    let value = cfg
        .queue
        .done_file
        .clone()
        .unwrap_or_else(|| PathBuf::from(".ralph/done.yaml"));
    if value.as_os_str().is_empty() {
        bail!("queue.done_file must be non-empty when set");
    }
    if value.is_absolute() {
        return Ok(value);
    }
    Ok(repo_root.join(value))
}

fn global_config_path() -> Option<PathBuf> {
    let base = if let Some(value) = env::var_os("XDG_CONFIG_HOME") {
        PathBuf::from(value)
    } else {
        let home = env::var_os("HOME")?;
        PathBuf::from(home).join(".config")
    };
    Some(base.join("ralph").join("config.yaml"))
}

fn project_config_path(repo_root: &Path) -> PathBuf {
    repo_root.join(".ralph").join("config.yaml")
}

fn find_repo_root(start: &Path) -> PathBuf {
    for dir in start.ancestors() {
        let ralph_dir = dir.join(".ralph");
        if ralph_dir.is_dir()
            && (ralph_dir.join("queue.yaml").is_file() || ralph_dir.join("config.yaml").is_file())
        {
            return dir.to_path_buf();
        }
        if dir.join(".git").exists() {
            return dir.to_path_buf();
        }
    }
    start.to_path_buf()
}
