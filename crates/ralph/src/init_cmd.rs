use crate::config;
use crate::contracts::{Config, QueueFile};
use crate::fsutil;
use crate::prompts;
use crate::queue;
use anyhow::{Context, Result};
use std::fs;
use std::path::Path;

const DEFAULT_RALPH_README: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/assets/ralph_readme.md"
));

pub struct InitOptions {
    pub force: bool,
    pub force_lock: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileInitStatus {
    Created,
    Valid,
}

pub struct InitReport {
    pub queue_status: FileInitStatus,
    pub done_status: FileInitStatus,
    pub config_status: FileInitStatus,
    pub readme_status: Option<FileInitStatus>,
}

pub fn run_init(resolved: &config::Resolved, opts: InitOptions) -> Result<InitReport> {
    let ralph_dir = resolved.repo_root.join(".ralph");
    fs::create_dir_all(&ralph_dir).with_context(|| format!("create {}", ralph_dir.display()))?;

    let _queue_lock = queue::acquire_queue_lock(&resolved.repo_root, "init", opts.force_lock)?;

    let queue_status = write_queue(
        &resolved.queue_path,
        opts.force,
        &resolved.id_prefix,
        resolved.id_width,
    )?;
    let done_status = write_done(
        &resolved.done_path,
        opts.force,
        &resolved.id_prefix,
        resolved.id_width,
    )?;
    let config_path = resolved
        .project_config_path
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("project config path unavailable"))?;
    let config_status = write_config(config_path, opts.force)?;

    let mut readme_status = None;
    if prompts::prompts_reference_readme(&resolved.repo_root)? {
        let readme_path = resolved.repo_root.join(".ralph/README.md");
        readme_status = Some(write_readme(&readme_path, opts.force)?);
    }

    Ok(InitReport {
        queue_status,
        done_status,
        config_status,
        readme_status,
    })
}

fn write_queue(
    path: &Path,
    force: bool,
    _id_prefix: &str,
    _id_width: usize,
) -> Result<FileInitStatus> {
    if path.exists() && !force {
        // Validate existing file by trying to load it
        let _queue = queue::load_queue(path)?;
        return Ok(FileInitStatus::Valid);
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
    }
    let queue = QueueFile::default();
    let rendered = serde_json::to_string_pretty(&queue).context("serialize queue JSON")?;
    fsutil::write_atomic(path, rendered.as_bytes())
        .with_context(|| format!("write queue JSON {}", path.display()))?;
    Ok(FileInitStatus::Created)
}

fn write_done(
    path: &Path,
    force: bool,
    _id_prefix: &str,
    _id_width: usize,
) -> Result<FileInitStatus> {
    if path.exists() && !force {
        // Validate existing file by trying to load it
        let _queue = queue::load_queue(path)?;
        return Ok(FileInitStatus::Valid);
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
    }
    let queue = QueueFile::default();
    let rendered = serde_json::to_string_pretty(&queue).context("serialize done JSON")?;
    fsutil::write_atomic(path, rendered.as_bytes())
        .with_context(|| format!("write done JSON {}", path.display()))?;
    Ok(FileInitStatus::Created)
}

fn write_config(path: &Path, force: bool) -> Result<FileInitStatus> {
    if path.exists() && !force {
        // Validate existing config by trying to parse it
        let raw =
            fs::read_to_string(path).with_context(|| format!("read config {}", path.display()))?;
        serde_json::from_str::<Config>(&raw).with_context(|| {
            format!(
                "Config file exists but is invalid JSON: {}. Use --force to overwrite.",
                path.display()
            )
        })?;
        return Ok(FileInitStatus::Valid);
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
    }
    let cfg = Config::default();
    let rendered = serde_json::to_string_pretty(&cfg).context("serialize config JSON")?;
    fsutil::write_atomic(path, rendered.as_bytes())
        .with_context(|| format!("write config JSON {}", path.display()))?;
    Ok(FileInitStatus::Created)
}

fn write_readme(path: &Path, force: bool) -> Result<FileInitStatus> {
    if path.exists() && !force {
        return Ok(FileInitStatus::Valid);
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
    }
    fsutil::write_atomic(path, DEFAULT_RALPH_README.as_bytes())
        .with_context(|| format!("write readme {}", path.display()))?;
    Ok(FileInitStatus::Created)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contracts::ProjectType;
    use tempfile::TempDir;

    fn resolved_for(dir: &TempDir) -> config::Resolved {
        let repo_root = dir.path().to_path_buf();
        let queue_path = repo_root.join(".ralph/queue.json");
        let done_path = repo_root.join(".ralph/done.json");
        let project_config_path = Some(repo_root.join(".ralph/config.json"));
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
        assert_eq!(report.queue_status, FileInitStatus::Created);
        assert_eq!(report.done_status, FileInitStatus::Created);
        assert_eq!(report.config_status, FileInitStatus::Created);
        assert_eq!(report.readme_status, Some(FileInitStatus::Created));
        let queue = crate::queue::load_queue(&resolved.queue_path)?;
        assert_eq!(queue.version, 1);
        let done = crate::queue::load_queue(&resolved.done_path)?;
        assert_eq!(done.version, 1);
        let raw_cfg = std::fs::read_to_string(resolved.project_config_path.as_ref().unwrap())?;
        let cfg: Config = serde_json::from_str(&raw_cfg)?;
        assert_eq!(cfg.version, 1);
        let readme_path = resolved.repo_root.join(".ralph/README.md");
        assert!(readme_path.exists());
        let readme_raw = std::fs::read_to_string(readme_path)?;
        assert!(readme_raw.contains("# Ralph runtime files"));
        Ok(())
    }

    #[test]
    fn init_skips_existing_when_not_forced() -> Result<()> {
        let dir = TempDir::new()?;
        let resolved = resolved_for(&dir);
        std::fs::create_dir_all(resolved.repo_root.join(".ralph"))?;
        let queue_json = r#"{
  "version": 1,
  "tasks": [
    {
      "id": "RQ-0001",
      "status": "todo",
      "title": "Keep",
      "tags": ["code"],
      "scope": ["x"],
      "evidence": ["y"],
      "plan": ["z"],
      "request": "test",
      "created_at": "2026-01-18T00:00:00Z",
      "updated_at": "2026-01-18T00:00:00Z"
    }
  ]
}"#;
        std::fs::write(&resolved.queue_path, queue_json)?;
        let done_json = r#"{
  "version": 1,
  "tasks": [
    {
      "id": "RQ-0002",
      "status": "done",
      "title": "Done",
      "tags": ["code"],
      "scope": ["x"],
      "evidence": ["y"],
      "plan": ["z"],
      "request": "test",
      "created_at": "2026-01-18T00:00:00Z",
      "updated_at": "2026-01-18T00:00:00Z"
    }
  ]
}"#;
        std::fs::write(&resolved.done_path, done_json)?;
        let config_json = r#"{
  "version": 1,
  "queue": {
    "file": ".ralph/queue.json"
  }
}"#;
        std::fs::write(resolved.project_config_path.as_ref().unwrap(), config_json)?;
        let report = run_init(
            &resolved,
            InitOptions {
                force: false,
                force_lock: false,
            },
        )?;
        assert_eq!(report.queue_status, FileInitStatus::Valid);
        assert_eq!(report.done_status, FileInitStatus::Valid);
        assert_eq!(report.config_status, FileInitStatus::Valid);
        assert_eq!(report.readme_status, Some(FileInitStatus::Created));
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
        std::fs::write(&resolved.queue_path, r#"{"version":1,"tasks":[]}"#)?;
        std::fs::write(&resolved.done_path, r#"{"version":1,"tasks":[]}"#)?;
        std::fs::write(
            resolved.project_config_path.as_ref().unwrap(),
            r#"{"version":1,"project_type":"docs"}"#,
        )?;
        let report = run_init(
            &resolved,
            InitOptions {
                force: true,
                force_lock: false,
            },
        )?;
        assert_eq!(report.queue_status, FileInitStatus::Created);
        assert_eq!(report.done_status, FileInitStatus::Created);
        assert_eq!(report.config_status, FileInitStatus::Created);
        assert_eq!(report.readme_status, Some(FileInitStatus::Created));
        let cfg_raw = std::fs::read_to_string(resolved.project_config_path.as_ref().unwrap())?;
        let cfg: Config = serde_json::from_str(&cfg_raw)?;
        assert_eq!(cfg.project_type, Some(ProjectType::Code));
        assert_eq!(
            cfg.queue.file,
            Some(std::path::PathBuf::from(".ralph/queue.json"))
        );
        assert_eq!(
            cfg.queue.done_file,
            Some(std::path::PathBuf::from(".ralph/done.json"))
        );
        assert_eq!(cfg.queue.id_prefix, Some("RQ".to_string()));
        assert_eq!(cfg.queue.id_width, Some(4));
        assert_eq!(cfg.agent.runner, Some(crate::contracts::Runner::Claude));
        assert_eq!(
            cfg.agent.model,
            Some(crate::contracts::Model::Custom("sonnet".to_string()))
        );
        assert_eq!(
            cfg.agent.reasoning_effort,
            Some(crate::contracts::ReasoningEffort::Medium)
        );
        assert_eq!(cfg.agent.gemini_bin, Some("gemini".to_string()));
        Ok(())
    }

    #[test]
    fn init_creates_json_for_new_install() -> Result<()> {
        let dir = TempDir::new()?;
        let resolved = resolved_for(&dir);
        let report = run_init(
            &resolved,
            InitOptions {
                force: false,
                force_lock: false,
            },
        )?;
        assert_eq!(report.queue_status, FileInitStatus::Created);
        assert_eq!(report.done_status, FileInitStatus::Created);
        assert_eq!(report.config_status, FileInitStatus::Created);

        // Verify JSON files were created
        let queue_raw = std::fs::read_to_string(&resolved.queue_path)?;
        assert!(queue_raw.contains("{"));
        let done_raw = std::fs::read_to_string(&resolved.done_path)?;
        assert!(done_raw.contains("{"));
        let cfg_raw = std::fs::read_to_string(resolved.project_config_path.as_ref().unwrap())?;
        assert!(cfg_raw.contains("{"));
        Ok(())
    }

    #[test]
    fn init_skips_readme_when_not_referenced() -> Result<()> {
        let dir = TempDir::new()?;
        let resolved = resolved_for(&dir);

        // Override worker prompt to NOT reference readme
        let overrides = resolved.repo_root.join(".ralph/prompts");
        fs::create_dir_all(&overrides)?;
        fs::write(overrides.join("worker.md"), "no reference")?;
        fs::write(overrides.join("task_builder.md"), "no reference")?;
        fs::write(overrides.join("scan.md"), "no reference")?;

        let report = run_init(
            &resolved,
            InitOptions {
                force: false,
                force_lock: false,
            },
        )?;
        assert_eq!(report.readme_status, None);
        let readme_path = resolved.repo_root.join(".ralph/README.md");
        assert!(!readme_path.exists());
        Ok(())
    }
}
