use crate::config;
use crate::contracts::{Config, QueueFile};
use crate::fsutil;
use crate::queue;
use anyhow::{Context, Result};
use std::fs;
use std::path::Path;

pub struct InitOptions {
    pub force: bool,
    pub force_lock: bool,
}

pub struct InitReport {
    pub queue_created: bool,
    pub done_created: bool,
    pub config_created: bool,
}

pub fn run_init(resolved: &config::Resolved, opts: InitOptions) -> Result<InitReport> {
    let ralph_dir = resolved.repo_root.join(".ralph");
    fs::create_dir_all(&ralph_dir).with_context(|| format!("create {}", ralph_dir.display()))?;

    let _queue_lock = queue::acquire_queue_lock(&resolved.repo_root, "init", opts.force_lock)?;

    let queue_created = write_queue(&resolved.queue_path, opts.force)?;
    let done_created = write_done(&resolved.done_path, opts.force)?;
    let config_path = resolved
        .project_config_path
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("project config path unavailable"))?;
    let config_created = write_config(config_path, opts.force)?;

    Ok(InitReport {
        queue_created,
        done_created,
        config_created,
    })
}

fn write_queue(path: &Path, force: bool) -> Result<bool> {
    if path.exists() && !force {
        return Ok(false);
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
    }
    let queue = QueueFile::default();
    let rendered = serde_yaml::to_string(&queue).context("serialize queue YAML")?;
    fsutil::write_atomic(path, rendered.as_bytes())
        .with_context(|| format!("write queue YAML {}", path.display()))?;
    Ok(true)
}

fn write_done(path: &Path, force: bool) -> Result<bool> {
    if path.exists() && !force {
        return Ok(false);
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
    }
    let queue = QueueFile::default();
    let rendered = serde_yaml::to_string(&queue).context("serialize done YAML")?;
    fsutil::write_atomic(path, rendered.as_bytes())
        .with_context(|| format!("write done YAML {}", path.display()))?;
    Ok(true)
}

fn write_config(path: &Path, force: bool) -> Result<bool> {
    if path.exists() && !force {
        return Ok(false);
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
    }
    let cfg = Config::default();
    let rendered = serde_yaml::to_string(&cfg).context("serialize config YAML")?;
    fsutil::write_atomic(path, rendered.as_bytes())
        .with_context(|| format!("write config YAML {}", path.display()))?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contracts::ProjectType;
    use tempfile::TempDir;

    fn resolved_for(dir: &TempDir) -> config::Resolved {
        let repo_root = dir.path().to_path_buf();
        let queue_path = repo_root.join(".ralph/queue.yaml");
        let done_path = repo_root.join(".ralph/done.yaml");
        let project_config_path = Some(repo_root.join(".ralph/config.yaml"));
        config::Resolved {
            config: Config::default(),
            repo_root,
            queue_path,
            done_path,
            id_prefix: "RQ".to_string(),
            id_width: 4,
            global_config_path: None,
            project_config_path,
        }
    }

    #[test]
    fn init_creates_missing_files() -> Result<()> {
        let dir = TempDir::new()?;
        let resolved = resolved_for(&dir);
        let report = run_init(
            &resolved,
            InitOptions {
                force: false,
                force_lock: false,
            },
        )?;
        assert!(report.queue_created);
        assert!(report.done_created);
        assert!(report.config_created);
        let (queue, repaired_queue) = crate::queue::load_queue_with_repair(&resolved.queue_path)?;
        assert!(!repaired_queue);
        assert_eq!(queue.version, 1);
        let (done, repaired_done) = crate::queue::load_queue_with_repair(&resolved.done_path)?;
        assert!(!repaired_done);
        assert_eq!(done.version, 1);
        let raw_cfg = std::fs::read_to_string(resolved.project_config_path.as_ref().unwrap())?;
        let cfg: Config = serde_yaml::from_str(&raw_cfg)?;
        assert_eq!(cfg.version, 1);
        Ok(())
    }

    #[test]
    fn init_skips_existing_when_not_forced() -> Result<()> {
        let dir = TempDir::new()?;
        let resolved = resolved_for(&dir);
        std::fs::create_dir_all(resolved.repo_root.join(".ralph"))?;
        std::fs::write(&resolved.queue_path, "version: 1\ntasks:\n  - id: RQ-0001\n    status: todo\n    title: Keep\n    tags: [code]\n    scope: [x]\n    evidence: [y]\n    plan: [z]\n")?;
        std::fs::write(&resolved.done_path, "version: 1\ntasks:\n  - id: RQ-0002\n    status: done\n    title: Done\n    tags: [code]\n    scope: [x]\n    evidence: [y]\n    plan: [z]\n")?;
        std::fs::write(
            resolved.project_config_path.as_ref().unwrap(),
            "version: 1\nqueue:\n  file: .ralph/queue.yaml\n",
        )?;
        let report = run_init(
            &resolved,
            InitOptions {
                force: false,
                force_lock: false,
            },
        )?;
        assert!(!report.queue_created);
        assert!(!report.done_created);
        assert!(!report.config_created);
        let raw = std::fs::read_to_string(&resolved.queue_path)?;
        assert!(raw.contains("Keep"));
        let done_raw = std::fs::read_to_string(&resolved.done_path)?;
        assert!(done_raw.contains("Done"));
        Ok(())
    }

    #[test]
    fn init_overwrites_when_forced() -> Result<()> {
        let dir = TempDir::new()?;
        let resolved = resolved_for(&dir);
        std::fs::create_dir_all(resolved.repo_root.join(".ralph"))?;
        std::fs::write(&resolved.queue_path, "version: 1\ntasks: []\n")?;
        std::fs::write(&resolved.done_path, "version: 1\ntasks: []\n")?;
        std::fs::write(
            resolved.project_config_path.as_ref().unwrap(),
            "version: 1\nproject_type: docs\n",
        )?;
        let report = run_init(
            &resolved,
            InitOptions {
                force: true,
                force_lock: false,
            },
        )?;
        assert!(report.queue_created);
        assert!(report.done_created);
        assert!(report.config_created);
        let cfg_raw = std::fs::read_to_string(resolved.project_config_path.as_ref().unwrap())?;
        let cfg: Config = serde_yaml::from_str(&cfg_raw)?;
        assert_eq!(cfg.project_type, Some(ProjectType::Code));
        assert_eq!(
            cfg.queue.file,
            Some(std::path::PathBuf::from(".ralph/queue.yaml"))
        );
        assert_eq!(
            cfg.queue.done_file,
            Some(std::path::PathBuf::from(".ralph/done.yaml"))
        );
        assert_eq!(cfg.queue.id_prefix, Some("RQ".to_string()));
        assert_eq!(cfg.queue.id_width, Some(4));
        assert_eq!(cfg.agent.runner, Some(crate::contracts::Runner::Codex));
        assert_eq!(cfg.agent.model, Some(crate::contracts::Model::Gpt52Codex));
        assert_eq!(
            cfg.agent.reasoning_effort,
            Some(crate::contracts::ReasoningEffort::Medium)
        );
        assert_eq!(cfg.agent.gemini_bin, Some("gemini".to_string()));
        Ok(())
    }
}
