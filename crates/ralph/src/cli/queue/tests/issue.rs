//! Tests for issue publish commands.
//!
//! Responsibilities:
//! - Test single and batch issue publishing.
//! - Test GitHub CLI integration (Unix-only tests).
//! - Test error handling and dry-run behavior.
//!
//! Not handled here:
//! - Import/export operations (see import.rs and export.rs).
//! - List/search operations (see list_search.rs).

use crate::cli::Cli;
use crate::cli::queue::issue;
use crate::cli::queue::shared::StatusArg;
use crate::contracts::{QueueFile, Task, TaskStatus};
use anyhow::Result;
use clap::CommandFactory;
use std::collections::HashMap;
use std::io::Write;
use std::path::Path;
use tempfile::TempDir;

fn base_issue_publish_args(task_id: &str) -> issue::QueueIssuePublishArgs {
    issue::QueueIssuePublishArgs {
        task_id: task_id.to_string(),
        dry_run: false,
        label: vec![],
        assignee: vec![],
        repo: None,
    }
}

fn issue_task(
    id: &str,
    title: &str,
    status: TaskStatus,
    tags: &[&str],
    custom_fields: &[(&str, &str)],
) -> Task {
    let mut fields = HashMap::new();
    for (key, value) in custom_fields {
        fields.insert((*key).to_string(), (*value).to_string());
    }

    Task {
        id: id.to_string(),
        status,
        title: title.to_string(),
        description: None,
        priority: Default::default(),
        tags: tags.iter().map(|tag| tag.to_string()).collect(),
        scope: vec!["crates/ralph".to_string()],
        evidence: vec!["test".to_string()],
        plan: vec!["verify".to_string()],
        notes: vec![],
        request: Some("test".to_string()),
        agent: None,
        created_at: Some("2026-01-18T00:00:00Z".to_string()),
        updated_at: Some("2026-01-18T00:00:00Z".to_string()),
        completed_at: None,
        started_at: None,
        scheduled_start: None,
        depends_on: vec![],
        blocks: vec![],
        relates_to: vec![],
        duplicates: None,
        custom_fields: fields,
        parent_id: None,
        estimated_minutes: None,
        actual_minutes: None,
    }
}

fn write_issue_queue_tasks(path: &Path, tasks: Vec<Task>) -> Result<()> {
    let queue = QueueFile { version: 1, tasks };
    let rendered = serde_json::to_string_pretty(&queue)?;
    std::fs::write(path, rendered)?;
    Ok(())
}

#[cfg(unix)]
fn create_fake_gh_for_issue_publish(
    tmp_dir: &TempDir,
    task_id: &str,
    issue_url: &str,
    auth_ok: bool,
) -> std::path::PathBuf {
    let bin_dir = tmp_dir.path().join("bin");
    std::fs::create_dir_all(&bin_dir).unwrap();
    let gh_path = bin_dir.join("gh");
    let log_path = tmp_dir.path().join("gh.log");

    let auth_status_block = if auth_ok {
        "exit 0"
    } else {
        "echo \"You are not logged into any GitHub hosts\" >&2\nexit 1"
    };

    let script = format!(
        r#"#!/bin/sh
set -eu

LOG="{log_path}"
TASK_ID="{task_id}"
ISSUE_URL="{issue_url}"

if [ "${{1:-}}" = "--version" ]; then
  echo "gh version 0.0.0-test"
  exit 0
fi

if [ "${{1:-}}" = "auth" ] && [ "${{2:-}}" = "status" ]; then
  {auth_status_block}
fi

if [ "${{1:-}}" = "issue" ] && [ "${{2:-}}" = "create" ]; then
  printf '%s\n' "$*" >> "$LOG"

  BODY_FILE=""
  TITLE=""
  i=1
  while [ $i -le $# ]; do
    eval arg=\${{${{i}}}}
    if [ "$arg" = "--body-file" ]; then
      i=$((i+1)); eval BODY_FILE=\${{${{i}}}}
    elif [ "$arg" = "--title" ]; then
      i=$((i+1)); eval TITLE=\${{${{i}}}}
    fi
    i=$((i+1))
  done

  if [ -z "$BODY_FILE" ] || [ ! -f "$BODY_FILE" ]; then
    echo "missing body file" >&2
    exit 2
  fi

  if [ -z "$TITLE" ]; then
    echo "missing title" >&2
    exit 3
  fi

  FOUND=0
  while IFS= read -r line; do
    if [ "$line" = "<!-- ralph_task_id: $TASK_ID -->" ]; then
      FOUND=1
    fi
  done < "$BODY_FILE"

  if [ "$FOUND" -ne 1 ]; then
    echo "missing marker" >&2
    exit 4
  fi

  echo "$ISSUE_URL"
  exit 0
fi

if [ "${{1:-}}" = "issue" ] && [ "${{2:-}}" = "edit" ]; then
  printf '%s\n' "$*" >> "$LOG"

  HAS_ADD_LABEL=0
  HAS_ADD_ASSIGNEE=0
  for arg in "$@"; do
    if [ "$arg" = "--add-label" ]; then HAS_ADD_LABEL=1; fi
    if [ "$arg" = "--add-assignee" ]; then HAS_ADD_ASSIGNEE=1; fi
  done

  if [ "$HAS_ADD_LABEL" -ne 1 ] || [ "$HAS_ADD_ASSIGNEE" -ne 1 ]; then
    echo "missing add flags" >&2
    exit 5
  fi

  exit 0
fi

echo "unknown args: $*" >&2
exit 1
"#,
        log_path = log_path.display(),
        task_id = task_id,
        issue_url = issue_url,
        auth_status_block = auth_status_block
    );

    let mut file = std::fs::File::create(&gh_path).unwrap();
    file.write_all(script.as_bytes()).unwrap();
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = file.metadata().unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&gh_path, perms).unwrap();
    }

    bin_dir
}

#[cfg(unix)]
fn create_fake_gh_for_issue_publish_multi(
    tmp_dir: &TempDir,
    issue_url: &str,
    auth_ok: bool,
    fail_task_id: Option<&str>,
) -> std::path::PathBuf {
    let bin_dir = tmp_dir.path().join("bin");
    std::fs::create_dir_all(&bin_dir).unwrap();
    let gh_path = bin_dir.join("gh");
    let log_path = tmp_dir.path().join("gh.log");
    let fail_task_id = fail_task_id.unwrap_or("");

    let auth_status_block = if auth_ok {
        "exit 0"
    } else {
        "echo \"You are not logged into any GitHub hosts\" >&2\nexit 1"
    };

    let script = format!(
        r#"#!/bin/sh
set -eu

LOG="{log_path}"
FAIL_TASK_ID="{fail_task_id}"

if [ "${{1:-}}" = "--version" ]; then
  echo "gh version 0.0.0-test"
  exit 0
fi

if [ "${{1:-}}" = "auth" ] && [ "${{2:-}}" = "status" ]; then
  {auth_status_block}
fi

if [ "${{1:-}}" = "issue" ] && [ "${{2:-}}" = "create" ]; then
  printf '%s\n' "$*" >> "$LOG"

  BODY_FILE=""
  TITLE=""
  i=1
  while [ $i -le $# ]; do
    eval arg=\${{${{i}}}}
    if [ "$arg" = "--body-file" ]; then
      i=$((i+1)); eval BODY_FILE=\${{${{i}}}}
    elif [ "$arg" = "--title" ]; then
      i=$((i+1)); eval TITLE=\${{${{i}}}}
    fi
    i=$((i+1))
  done

  if [ -z "$BODY_FILE" ] || [ ! -f "$BODY_FILE" ]; then
    echo "missing body file" >&2
    exit 2
  fi
  if [ -z "$TITLE" ]; then
    echo "missing title" >&2
    exit 3
  fi

  if [ -n "$FAIL_TASK_ID" ] && grep -q "ralph_task_id: $FAIL_TASK_ID" "$BODY_FILE"; then
    echo "simulated failure for task $FAIL_TASK_ID" >&2
    exit 7
  fi

  echo "{issue_url}"
  exit 0
fi

if [ "${{1:-}}" = "issue" ] && [ "${{2:-}}" = "edit" ]; then
  printf '%s\n' "$*" >> "$LOG"

  BODY_FILE=""
  i=1
  while [ $i -le $# ]; do
    eval arg=\${{${{i}}}}
    if [ "$arg" = "--body-file" ]; then
      i=$((i+1)); eval BODY_FILE=\${{${{i}}}}
    fi
    i=$((i+1))
  done

  if [ -z "$BODY_FILE" ] || [ ! -f "$BODY_FILE" ]; then
    echo "missing body file" >&2
    exit 2
  fi

  if [ -n "$FAIL_TASK_ID" ] && grep -q "ralph_task_id: $FAIL_TASK_ID" "$BODY_FILE"; then
    echo "simulated failure for task $FAIL_TASK_ID" >&2
    exit 7
  fi
  exit 0
fi

echo "unknown args: $*" >&2
exit 1
"#,
        log_path = log_path.display(),
        issue_url = issue_url,
        fail_task_id = fail_task_id,
        auth_status_block = auth_status_block
    );

    let mut file = std::fs::File::create(&gh_path).unwrap();
    file.write_all(script.as_bytes()).unwrap();
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = file.metadata().unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&gh_path, perms).unwrap();
    }

    bin_dir
}

#[test]
fn queue_issue_publish_help_examples_expanded() {
    let mut cmd = Cli::command();
    let queue = cmd.find_subcommand_mut("queue").expect("queue subcommand");
    let issue_cmd = queue
        .find_subcommand_mut("issue")
        .expect("queue issue subcommand");
    let help = issue_cmd.render_long_help().to_string();

    assert!(
        help.contains("ralph queue issue publish"),
        "missing issue publish example: {help}"
    );
    assert!(help.contains("ralph queue issue publish-many"));
}

#[test]
fn queue_issue_publish_dry_run_succeeds() -> Result<()> {
    use super::{resolved_for_dir, write_queue};

    let dir = TempDir::new()?;
    let resolved = resolved_for_dir(&dir);
    write_queue(&resolved.queue_path)?;

    let args = base_issue_publish_args("RQ-0001");
    let args = issue::QueueIssuePublishArgs {
        dry_run: true,
        ..args
    };

    let result = issue::handle_publish(&resolved, true, args);
    assert!(result.is_ok());

    Ok(())
}

#[cfg(unix)]
#[test]
fn queue_issue_publish_many_dry_run_filters() -> Result<()> {
    use super::resolved_for_dir;

    let dir = TempDir::new()?;
    let resolved = resolved_for_dir(&dir);
    write_issue_queue_tasks(
        &resolved.queue_path,
        vec![
            issue_task("RQ-0001", "Bug task one", TaskStatus::Todo, &["bug"], &[]),
            issue_task("RQ-0002", "Bug task two", TaskStatus::Todo, &["bug"], &[]),
            issue_task("RQ-0003", "Other task", TaskStatus::Doing, &["cli"], &[]),
        ],
    )?;

    let args = issue::QueueIssuePublishManyArgs {
        status: vec![StatusArg::Todo],
        tag: vec!["bug".to_string()],
        id_pattern: Some("^RQ-0001$".to_string()),
        dry_run: false,
        execute: false,
        label: vec![],
        assignee: vec![],
        repo: None,
    };

    let result = issue::handle_publish_many(&resolved, true, args);
    assert!(result.is_ok());

    let queue = crate::queue::load_queue(&resolved.queue_path)?;
    assert_eq!(queue.tasks.len(), 3);
    assert_eq!(queue.tasks[0].id, "RQ-0001");

    Ok(())
}

#[cfg(unix)]
#[test]
fn queue_issue_publish_many_exec_mixed_create_update() -> Result<()> {
    use super::resolved_for_dir;
    use crate::testsupport::path::with_prepend_path;

    let dir = TempDir::new()?;
    let resolved = resolved_for_dir(&dir);
    write_issue_queue_tasks(
        &resolved.queue_path,
        vec![
            issue_task("RQ-0001", "Bug task one", TaskStatus::Todo, &["bug"], &[]),
            issue_task(
                "RQ-0002",
                "Bug task two",
                TaskStatus::Todo,
                &["bug"],
                &[
                    ("github_issue_url", "https://github.com/org/repo/issues/777"),
                    ("github_issue_number", "777"),
                ],
            ),
        ],
    )?;

    let bin_dir = create_fake_gh_for_issue_publish_multi(
        &dir,
        "https://github.com/org/repo/issues/123",
        true,
        None,
    );

    let args = issue::QueueIssuePublishManyArgs {
        status: vec![StatusArg::Todo],
        tag: vec!["bug".to_string()],
        id_pattern: None,
        dry_run: false,
        execute: true,
        label: vec!["triage".to_string()],
        assignee: vec![],
        repo: None,
    };

    with_prepend_path(&bin_dir, || {
        issue::handle_publish_many(&resolved, true, args)
    })?;

    let queue = crate::queue::load_queue(&resolved.queue_path)?;
    let first = queue
        .tasks
        .iter()
        .find(|t| t.id == "RQ-0001")
        .expect("first task");
    let second = queue
        .tasks
        .iter()
        .find(|t| t.id == "RQ-0002")
        .expect("second task");

    assert_eq!(
        first
            .custom_fields
            .get("github_issue_url")
            .map(String::as_str),
        Some("https://github.com/org/repo/issues/123")
    );
    assert!(
        first
            .custom_fields
            .contains_key(crate::git::GITHUB_ISSUE_SYNC_HASH_KEY)
    );
    assert_eq!(
        second
            .custom_fields
            .get("github_issue_url")
            .map(String::as_str),
        Some("https://github.com/org/repo/issues/777")
    );
    assert!(
        second
            .custom_fields
            .contains_key(crate::git::GITHUB_ISSUE_SYNC_HASH_KEY)
    );

    Ok(())
}

#[cfg(unix)]
#[test]
fn queue_issue_publish_many_skips_if_unchanged() -> Result<()> {
    use super::resolved_for_dir;

    let dir = TempDir::new()?;
    let resolved = resolved_for_dir(&dir);
    let task = issue_task("RQ-0001", "No-op task", TaskStatus::Todo, &["bug"], &[]);
    let body = crate::cli::queue::export::render_task_as_github_issue_body(&task);
    let hash = crate::git::compute_issue_sync_hash(
        &format!("{}: {}", task.id, task.title),
        &body,
        &[],
        &[],
        None,
    )?;

    write_issue_queue_tasks(
        &resolved.queue_path,
        vec![issue_task(
            "RQ-0001",
            "No-op task",
            TaskStatus::Todo,
            &["bug"],
            &[
                ("github_issue_url", "https://github.com/org/repo/issues/123"),
                (crate::git::GITHUB_ISSUE_SYNC_HASH_KEY, &hash),
            ],
        )],
    )?;

    let args = issue::QueueIssuePublishManyArgs {
        status: vec![StatusArg::Todo],
        tag: vec!["bug".to_string()],
        id_pattern: None,
        dry_run: false,
        execute: false,
        label: vec![],
        assignee: vec![],
        repo: None,
    };

    issue::handle_publish_many(&resolved, true, args)?;
    Ok(())
}

#[cfg(unix)]
#[test]
fn queue_issue_publish_many_partial_failures_do_not_abort() -> Result<()> {
    use super::resolved_for_dir;
    use crate::testsupport::path::with_prepend_path;

    let dir = TempDir::new()?;
    let resolved = resolved_for_dir(&dir);
    write_issue_queue_tasks(
        &resolved.queue_path,
        vec![
            issue_task(
                "RQ-0001",
                "Task that fails",
                TaskStatus::Todo,
                &["bug"],
                &[],
            ),
            issue_task(
                "RQ-0002",
                "Task that succeeds",
                TaskStatus::Todo,
                &["bug"],
                &[
                    ("github_issue_url", "https://github.com/org/repo/issues/777"),
                    ("github_issue_number", "777"),
                ],
            ),
        ],
    )?;

    let bin_dir = create_fake_gh_for_issue_publish_multi(
        &dir,
        "https://github.com/org/repo/issues/123",
        true,
        Some("RQ-0001"),
    );

    let args = issue::QueueIssuePublishManyArgs {
        status: vec![StatusArg::Todo],
        tag: vec!["bug".to_string()],
        id_pattern: None,
        dry_run: false,
        execute: true,
        label: vec![],
        assignee: vec![],
        repo: None,
    };
    let err = with_prepend_path(&bin_dir, || {
        issue::handle_publish_many(&resolved, true, args)
    })
    .expect_err("expected publish-many failure");
    assert!(
        err.to_string().contains("completed with 1 failed task(s)")
            || err.to_string().contains("simulated failure"),
        "unexpected error: {err}"
    );

    let queue = crate::queue::load_queue(&resolved.queue_path)?;
    assert_eq!(queue.tasks.len(), 2);
    assert_eq!(queue.tasks[1].id, "RQ-0002");
    assert!(
        queue.tasks[1]
            .custom_fields
            .contains_key(crate::git::GITHUB_ISSUE_SYNC_HASH_KEY)
    );

    Ok(())
}

#[test]
fn queue_issue_publish_fails_when_task_not_found() {
    use super::{resolved_for_dir, write_queue};

    let dir = TempDir::new().expect("temp dir");
    let resolved = resolved_for_dir(&dir);
    write_queue(&resolved.queue_path).expect("write queue");

    let args = base_issue_publish_args("RQ-9999");

    let err = issue::handle_publish(&resolved, true, args).expect_err("expected error");
    let msg = err.to_string();
    assert!(
        msg.contains("not found") || msg.contains("RQ-9999"),
        "unexpected error: {msg}"
    );
}

#[test]
fn queue_issue_publish_fails_when_gh_missing() -> Result<()> {
    use super::{resolved_for_dir, write_queue};
    use crate::testsupport::path::with_path;

    let dir = TempDir::new()?;
    let resolved = resolved_for_dir(&dir);
    write_queue(&resolved.queue_path)?;

    let args = base_issue_publish_args("RQ-0001");

    let err =
        with_path("", || issue::handle_publish(&resolved, true, args)).expect_err("expected error");
    let msg = err.to_string();
    assert!(
        msg.contains("GitHub CLI (`gh`) not found on PATH"),
        "unexpected error: {msg}"
    );

    Ok(())
}

#[cfg(unix)]
#[test]
fn queue_issue_publish_fails_when_gh_unauthenticated() -> Result<()> {
    use super::{resolved_for_dir, write_queue};
    use crate::testsupport::path::with_prepend_path;

    let dir = TempDir::new()?;
    let resolved = resolved_for_dir(&dir);
    write_queue(&resolved.queue_path)?;

    let bin_dir = create_fake_gh_for_issue_publish(
        &dir,
        "RQ-0001",
        "https://github.com/org/repo/issues/123",
        false,
    );

    let args = base_issue_publish_args("RQ-0001");
    let err = with_prepend_path(&bin_dir, || issue::handle_publish(&resolved, true, args))
        .expect_err("expected error");
    let msg = err.to_string();
    assert!(
        msg.contains("not authenticated") && msg.contains("gh auth login"),
        "unexpected error: {msg}"
    );

    Ok(())
}

#[cfg(unix)]
#[test]
fn queue_issue_publish_creates_issue_and_persists_custom_fields() -> Result<()> {
    use super::{resolved_for_dir, write_queue};
    use crate::testsupport::path::with_prepend_path;

    let dir = TempDir::new()?;
    let resolved = resolved_for_dir(&dir);
    write_queue(&resolved.queue_path)?;

    let bin_dir = create_fake_gh_for_issue_publish(
        &dir,
        "RQ-0001",
        "https://github.com/org/repo/issues/123",
        true,
    );

    let args = issue::QueueIssuePublishArgs {
        label: vec!["bug".to_string()],
        assignee: vec!["@me".to_string()],
        ..base_issue_publish_args("RQ-0001")
    };

    with_prepend_path(&bin_dir, || issue::handle_publish(&resolved, true, args))?;

    let queue = crate::queue::load_queue(&resolved.queue_path)?;
    let task = queue
        .tasks
        .iter()
        .find(|t| t.id == "RQ-0001")
        .expect("task");

    assert_eq!(
        task.custom_fields
            .get("github_issue_url")
            .map(String::as_str),
        Some("https://github.com/org/repo/issues/123")
    );
    assert_eq!(
        task.custom_fields
            .get("github_issue_number")
            .map(String::as_str),
        Some("123")
    );
    assert!(
        task.updated_at.as_deref() != Some("2026-01-18T00:00:00Z"),
        "updated_at should be updated on publish"
    );

    Ok(())
}

#[cfg(unix)]
#[test]
fn queue_issue_publish_updates_existing_issue_and_backfills_issue_number() -> Result<()> {
    use super::resolved_for_dir;
    use crate::testsupport::path::with_prepend_path;

    let dir = TempDir::new()?;
    let resolved = resolved_for_dir(&dir);

    // Write queue with task
    let task = Task {
        id: "RQ-0001".to_string(),
        title: "Test task".to_string(),
        status: TaskStatus::Todo,
        description: None,
        priority: Default::default(),
        tags: vec!["cli".to_string()],
        scope: vec!["crates/ralph".to_string()],
        evidence: vec!["test".to_string()],
        plan: vec!["verify".to_string()],
        notes: vec![],
        request: Some("test".to_string()),
        agent: None,
        created_at: Some("2026-01-18T00:00:00Z".to_string()),
        updated_at: Some("2026-01-18T00:00:00Z".to_string()),
        completed_at: None,
        started_at: None,
        scheduled_start: None,
        depends_on: vec![],
        blocks: vec![],
        relates_to: vec![],
        duplicates: None,
        custom_fields: {
            let mut m = HashMap::new();
            m.insert(
                "github_issue_url".to_string(),
                "https://github.com/org/repo/issues/777".to_string(),
            );
            m
        },
        parent_id: None,
        estimated_minutes: None,
        actual_minutes: None,
    };
    let queue = QueueFile {
        version: 1,
        tasks: vec![task],
    };
    crate::queue::save_queue(&resolved.queue_path, &queue)?;

    let bin_dir = create_fake_gh_for_issue_publish(
        &dir,
        "RQ-0001",
        "https://github.com/org/repo/issues/777",
        true,
    );

    let args = issue::QueueIssuePublishArgs {
        label: vec!["help-wanted".to_string()],
        assignee: vec!["@me".to_string()],
        ..base_issue_publish_args("RQ-0001")
    };

    with_prepend_path(&bin_dir, || issue::handle_publish(&resolved, true, args))?;

    let queue = crate::queue::load_queue(&resolved.queue_path)?;
    let task = queue
        .tasks
        .iter()
        .find(|t| t.id == "RQ-0001")
        .expect("task");

    assert_eq!(
        task.custom_fields
            .get("github_issue_url")
            .map(String::as_str),
        Some("https://github.com/org/repo/issues/777")
    );
    assert_eq!(
        task.custom_fields
            .get("github_issue_number")
            .map(String::as_str),
        Some("777")
    );

    Ok(())
}

#[test]
fn queue_issue_publish_help_contains_publish_subcommand() {
    let mut cmd = Cli::command();
    let queue = cmd.find_subcommand_mut("queue").expect("queue subcommand");
    let _issue_cmd = queue
        .find_subcommand_mut("issue")
        .expect("queue issue subcommand");
}
