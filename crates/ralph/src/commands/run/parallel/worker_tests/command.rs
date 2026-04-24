//! Worker command construction tests.
//!
//! Purpose:
//! - Worker command construction tests.
//!
//! Responsibilities:
//! - Provide focused implementation or regression coverage for this file's owning feature.
//!
//! Scope:
//! - Limited to this file's owning feature boundary.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/Assumptions:
//! - Keep behavior aligned with Ralph's canonical CLI, machine-contract, and queue semantics.

use super::*;

#[test]
fn build_worker_command_sets_cwd_and_args() -> Result<()> {
    let temp = TempDir::new()?;
    let workspace_path = temp.path().join("workspace");
    std::fs::create_dir_all(&workspace_path)?;

    let ralph_dir = temp.path().join(".ralph");
    std::fs::create_dir_all(&ralph_dir)?;
    let resolved = config::Resolved {
        config: crate::contracts::Config::default(),
        repo_root: temp.path().to_path_buf(),
        queue_path: ralph_dir.join("queue.json"),
        done_path: ralph_dir.join("done.json"),
        id_prefix: "RQ".to_string(),
        id_width: 4,
        global_config_path: None,
        project_config_path: None,
    };

    let overrides = AgentOverrides::default();
    let cmd = build_worker_command(
        &resolved,
        &workspace_path,
        "RQ-1234",
        "main",
        &overrides,
        true,
    )?;
    let args = debug_command_args(&cmd);

    assert_eq!(cmd.get_current_dir(), Some(workspace_path.as_path()));

    let mut pwd_seen = false;
    for (key, value) in cmd.get_envs() {
        if key == std::ffi::OsStr::new("PWD") {
            pwd_seen = true;
            assert_eq!(value, Some(workspace_path.as_os_str()));
        }
    }
    assert!(pwd_seen, "PWD env should be set for workspace execution");

    assert!(args.contains(&"--force".to_string()));
    assert!(args.contains(&"--no-progress".to_string()));
    assert!(args.contains(&"run".to_string()));
    assert!(args.contains(&"one".to_string()));
    assert!(args.contains(&"--parallel-worker".to_string()));
    assert!(args.contains(&"--non-interactive".to_string()));
    // Default overrides should not emit git publish flags.
    assert!(!args.contains(&"--git-publish-mode".to_string()));
    // Default overrides should not emit runner/model/phase flags.
    // Workers must resolve these from workspace-local .ralph/config.jsonc.
    assert!(!args.contains(&"--runner".to_string()));
    assert!(!args.contains(&"--model".to_string()));
    assert!(!args.contains(&"--effort".to_string()));
    assert!(!args.contains(&"--phases".to_string()));
    assert!(!args.contains(&"--runner-phase1".to_string()));
    assert!(!args.contains(&"--runner-phase2".to_string()));
    assert!(!args.contains(&"--runner-phase3".to_string()));
    assert!(!args.contains(&"--model-phase1".to_string()));
    assert!(!args.contains(&"--model-phase2".to_string()));
    assert!(!args.contains(&"--model-phase3".to_string()));
    assert!(!args.contains(&"--effort-phase1".to_string()));
    assert!(!args.contains(&"--effort-phase2".to_string()));
    assert!(!args.contains(&"--effort-phase3".to_string()));

    let run_pos = args.iter().position(|arg| arg == "run").expect("run");
    let one_pos = args.iter().position(|arg| arg == "one").expect("one");
    let no_progress_pos = args
        .iter()
        .position(|arg| arg == "--no-progress")
        .expect("--no-progress");
    assert!(
        no_progress_pos > one_pos && one_pos > run_pos,
        "--no-progress must be scoped under `run one`, got args: {:?}",
        args
    );

    let id_pos = args.iter().position(|arg| arg == "--id").expect("--id");
    assert_eq!(args.get(id_pos + 1), Some(&"RQ-1234".to_string()));

    // Verify workspace queue/done paths are passed via CLI flags
    let expected_workspace_queue = workspace_path.join(".ralph").join("queue.json");
    let expected_workspace_done = workspace_path.join(".ralph").join("done.json");
    let queue_path_pos = args
        .iter()
        .position(|arg| arg == "--coordinator-queue-path")
        .expect("--coordinator-queue-path should be in args");
    assert_eq!(
        args.get(queue_path_pos + 1),
        Some(&expected_workspace_queue.to_string_lossy().to_string()),
        "workspace queue path should follow --coordinator-queue-path flag"
    );

    let done_path_pos = args
        .iter()
        .position(|arg| arg == "--coordinator-done-path")
        .expect("--coordinator-done-path should be in args");
    assert_eq!(
        args.get(done_path_pos + 1),
        Some(&expected_workspace_done.to_string_lossy().to_string()),
        "workspace done path should follow --coordinator-done-path flag"
    );

    let target_branch_pos = args
        .iter()
        .position(|arg| arg == "--parallel-target-branch")
        .expect("--parallel-target-branch should be in args");
    assert_eq!(
        args.get(target_branch_pos + 1),
        Some(&"main".to_string()),
        "target branch should follow --parallel-target-branch flag"
    );

    Ok(())
}

#[test]
fn build_worker_command_maps_custom_queue_done_paths_into_workspace() -> Result<()> {
    let temp = TempDir::new()?;
    let repo_root = temp.path().join("repo");
    let workspace_path = temp.path().join("workspace");
    std::fs::create_dir_all(&repo_root)?;
    std::fs::create_dir_all(&workspace_path)?;

    let resolved = config::Resolved {
        config: crate::contracts::Config::default(),
        repo_root: repo_root.clone(),
        queue_path: repo_root.join("queue/active.json"),
        done_path: repo_root.join("archive/done.json"),
        id_prefix: "RQ".to_string(),
        id_width: 4,
        global_config_path: None,
        project_config_path: None,
    };

    let overrides = AgentOverrides::default();
    let cmd = build_worker_command(
        &resolved,
        &workspace_path,
        "RQ-1234",
        "main",
        &overrides,
        false,
    )?;
    let args = debug_command_args(&cmd);

    let queue_path_pos = args
        .iter()
        .position(|arg| arg == "--coordinator-queue-path")
        .expect("--coordinator-queue-path should be in args");
    let done_path_pos = args
        .iter()
        .position(|arg| arg == "--coordinator-done-path")
        .expect("--coordinator-done-path should be in args");

    assert_eq!(
        args.get(queue_path_pos + 1),
        Some(
            &workspace_path
                .join("queue/active.json")
                .to_string_lossy()
                .to_string()
        )
    );
    assert_eq!(
        args.get(done_path_pos + 1),
        Some(
            &workspace_path
                .join("archive/done.json")
                .to_string_lossy()
                .to_string()
        )
    );
    Ok(())
}

#[test]
fn build_worker_command_emits_git_publish_mode_commit_and_push_when_overridden() -> Result<()> {
    let temp = TempDir::new()?;
    let workspace_path = temp.path().join("workspace");
    std::fs::create_dir_all(&workspace_path)?;

    let ralph_dir = temp.path().join(".ralph");
    std::fs::create_dir_all(&ralph_dir)?;
    let resolved = config::Resolved {
        config: crate::contracts::Config::default(),
        repo_root: temp.path().to_path_buf(),
        queue_path: ralph_dir.join("queue.json"),
        done_path: ralph_dir.join("done.json"),
        id_prefix: "RQ".to_string(),
        id_width: 4,
        global_config_path: None,
        project_config_path: None,
    };

    let overrides = AgentOverrides {
        git_publish_mode: Some(crate::contracts::GitPublishMode::CommitAndPush),
        ..Default::default()
    };
    let cmd = build_worker_command(
        &resolved,
        &workspace_path,
        "RQ-1234",
        "main",
        &overrides,
        false,
    )?;
    let args = debug_command_args(&cmd);

    assert!(args.contains(&"--git-publish-mode".to_string()));
    assert!(args.contains(&"commit_and_push".to_string()));

    Ok(())
}

#[test]
fn build_worker_command_emits_git_publish_mode_off_when_overridden() -> Result<()> {
    let temp = TempDir::new()?;
    let workspace_path = temp.path().join("workspace");
    std::fs::create_dir_all(&workspace_path)?;

    let ralph_dir = temp.path().join(".ralph");
    std::fs::create_dir_all(&ralph_dir)?;
    let resolved = config::Resolved {
        config: crate::contracts::Config::default(),
        repo_root: temp.path().to_path_buf(),
        queue_path: ralph_dir.join("queue.json"),
        done_path: ralph_dir.join("done.json"),
        id_prefix: "RQ".to_string(),
        id_width: 4,
        global_config_path: None,
        project_config_path: None,
    };

    let overrides = AgentOverrides {
        git_publish_mode: Some(crate::contracts::GitPublishMode::Off),
        ..Default::default()
    };
    let cmd = build_worker_command(
        &resolved,
        &workspace_path,
        "RQ-1234",
        "main",
        &overrides,
        false,
    )?;
    let args = debug_command_args(&cmd);

    assert!(args.contains(&"--git-publish-mode".to_string()));
    assert!(args.contains(&"off".to_string()));

    Ok(())
}
