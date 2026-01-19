use anyhow::{Context, Result};
use ralph::contracts::{QueueFile, Task, TaskStatus};
use ralph::queue;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use tempfile::TempDir;

const ID_WIDTH: usize = 5;
const ID_PREFIX: &str = "RQ";
const STRESS_TOTAL_TASKS: u32 = 10_000;
const STRESS_DONE_TASKS: u32 = 5_000;
const STRESS_RUNTIME_BOUND_SECS: u64 = 3;

#[derive(Clone, Copy)]
struct StressProfile {
    total_tasks: u32,
    done_tasks: u32,
    iterations: u32,
    archive_batch: u32,
}

fn make_task_with(id_num: u32, status: TaskStatus, id_prefix: &str, id_width: usize) -> Task {
    Task {
        id: format!("{id_prefix}-{id_num:0width$}", width = id_width),
        status,
        title: format!("Task {id_num}"),
        priority: Default::default(),
        tags: vec!["rust".to_string()],
        scope: vec!["crates/ralph".to_string()],
        evidence: vec!["stress fixture".to_string()],
        plan: vec!["exercise queue ops".to_string()],
        notes: vec![],
        request: Some("stress test".to_string()),
        agent: None,
        created_at: Some("2026-01-18T00:00:00Z".to_string()),
        updated_at: Some("2026-01-18T00:00:00Z".to_string()),
        completed_at: if status == TaskStatus::Done {
            Some("2026-01-18T00:00:00Z".to_string())
        } else {
            None
        },
        depends_on: vec![],
    }
}

fn build_queue(
    profile: &StressProfile,
    id_prefix: &str,
    id_width: usize,
) -> (QueueFile, QueueFile) {
    let mut active = QueueFile {
        version: 1,
        tasks: Vec::new(),
    };
    let mut done = QueueFile {
        version: 1,
        tasks: Vec::new(),
    };

    let done_target = profile.done_tasks.min(profile.total_tasks);
    for i in 1..=profile.total_tasks {
        if i <= done_target {
            done.tasks
                .push(make_task_with(i, TaskStatus::Done, id_prefix, id_width));
        } else {
            active
                .tasks
                .push(make_task_with(i, TaskStatus::Todo, id_prefix, id_width));
        }
    }

    (active, done)
}

fn make_task(id_num: u32, status: TaskStatus) -> Task {
    make_task_with(id_num, status, ID_PREFIX, ID_WIDTH)
}

fn write_queue_files(
    dir: &TempDir,
    active: &QueueFile,
    done: &QueueFile,
) -> Result<(PathBuf, PathBuf)> {
    let queue_path = dir.path().join("queue.json");
    let done_path = dir.path().join("done.json");
    queue::save_queue(&queue_path, active).with_context(|| "save active queue")?;
    queue::save_queue(&done_path, done).with_context(|| "save done queue")?;
    Ok((queue_path, done_path))
}

#[test]
fn stress_queue_ops_large_scale() -> Result<()> {
    let profile = StressProfile {
        total_tasks: STRESS_TOTAL_TASKS,
        done_tasks: STRESS_DONE_TASKS,
        iterations: 0,
        archive_batch: 0,
    };
    let (active, done) = build_queue(&profile, ID_PREFIX, ID_WIDTH);

    queue::validate_queue_set(&active, Some(&done), ID_PREFIX, ID_WIDTH)
        .context("validate queue set")?;

    let next = queue::next_id_across(&active, Some(&done), ID_PREFIX, ID_WIDTH)
        .context("next id across")?;
    anyhow::ensure!(next == "RQ-10001", "unexpected next id: {next}");

    let filtered = queue::filter_tasks(
        &active,
        &[TaskStatus::Todo],
        &["rust".to_string()],
        &["crates/ralph".to_string()],
        Some(5),
    );
    anyhow::ensure!(
        filtered.len() == 5,
        "unexpected filtered len: {}",
        filtered.len()
    );
    anyhow::ensure!(filtered[0].id == "RQ-05001", "unexpected first filtered id");
    anyhow::ensure!(filtered[4].id == "RQ-05005", "unexpected fifth filtered id");

    let found_active = queue::find_task_across(&active, Some(&done), "RQ-05001")
        .ok_or_else(|| anyhow::anyhow!("expected to find active task"))?;
    anyhow::ensure!(found_active.status == TaskStatus::Todo);

    let found_done = queue::find_task_across(&active, Some(&done), "RQ-00042")
        .ok_or_else(|| anyhow::anyhow!("expected to find done task"))?;
    anyhow::ensure!(found_done.status == TaskStatus::Done);

    // Serialize and parse roundtrip.
    let dir = TempDir::new().context("create temp dir")?;
    let (queue_path, done_path) = write_queue_files(&dir, &active, &done)?;

    let reloaded_active = queue::load_queue(&queue_path).context("load active")?;
    let reloaded_done = queue::load_queue(&done_path).context("load done")?;

    queue::validate_queue_set(&reloaded_active, Some(&reloaded_done), ID_PREFIX, ID_WIDTH)
        .context("validate reloaded queue set")?;

    Ok(())
}

#[test]
fn stress_queue_ops_runtime_bounds() -> Result<()> {
    let profile = StressProfile {
        total_tasks: STRESS_TOTAL_TASKS,
        done_tasks: STRESS_DONE_TASKS,
        iterations: 0,
        archive_batch: 0,
    };
    let (active, done) = build_queue(&profile, ID_PREFIX, ID_WIDTH);

    let start = Instant::now();
    queue::validate_queue_set(&active, Some(&done), ID_PREFIX, ID_WIDTH)?;
    let _ = queue::next_id_across(&active, Some(&done), ID_PREFIX, ID_WIDTH)?;
    let _ = queue::filter_tasks(&active, &[], &[], &[], Some(50));
    let elapsed = start.elapsed();

    let bound = Duration::from_secs(STRESS_RUNTIME_BOUND_SECS);
    anyhow::ensure!(
        elapsed <= bound,
        "stress ops exceeded bound: {:?} > {:?}",
        elapsed,
        bound
    );

    Ok(())
}

#[test]
fn stress_queue_archive_and_mutate_cycles() -> Result<()> {
    let profile = StressProfile {
        total_tasks: 2000,
        done_tasks: 0,
        iterations: 20,
        archive_batch: 25,
    };

    let (active, done) = build_queue(&profile, ID_PREFIX, ID_WIDTH);
    let dir = TempDir::new().context("create temp dir")?;
    let (queue_path, done_path) = write_queue_files(&dir, &active, &done)?;
    let now = "2026-01-18T00:00:00Z";

    for iter in 0..profile.iterations {
        let mut current = queue::load_queue(&queue_path).context("load active")?;
        let start = 1 + iter * profile.archive_batch;
        if start > profile.total_tasks {
            break;
        }
        let end = (start + profile.archive_batch).min(profile.total_tasks + 1);

        for id_num in start..end {
            let id = format!("{ID_PREFIX}-{id_num:0width$}", width = ID_WIDTH);
            let _ = queue::set_status(&mut current, &id, TaskStatus::Done, now, None);
        }

        queue::save_queue(&queue_path, &current).context("save active")?;
        let _report = queue::archive_done_tasks(&queue_path, &done_path, ID_PREFIX, ID_WIDTH)
            .with_context(|| format!("archive iteration {iter}"))?;

        let active_reloaded = queue::load_queue(&queue_path).context("reload active")?;
        let done_reloaded = queue::load_queue(&done_path).context("reload done")?;
        queue::validate_queue_set(&active_reloaded, Some(&done_reloaded), ID_PREFIX, ID_WIDTH)
            .context("validate after iteration")?;
    }

    Ok(())
}

#[test]
fn stress_queue_load_large_yaml_scalars() -> Result<()> {
    let dir = TempDir::new().context("create temp dir")?;
    let queue_path = dir.path().join("queue.json");

    // Build a large queue and save it
    let mut queue = QueueFile {
        version: 1,
        tasks: Vec::new(),
    };
    for i in 1..=2000 {
        queue
            .tasks
            .push(make_task_with(i, TaskStatus::Todo, ID_PREFIX, ID_WIDTH));
    }
    queue::save_queue(&queue_path, &queue)?;

    let reloaded = queue::load_queue(&queue_path)?;
    anyhow::ensure!(reloaded.tasks.len() == 2000, "unexpected task count");

    queue::validate_queue(&reloaded, ID_PREFIX, ID_WIDTH).context("validate loaded queue")?;

    Ok(())
}

#[test]
fn stress_queue_yaml_fallback() -> Result<()> {
    let dir = TempDir::new().context("create temp dir")?;
    let id_prefix = "RQ";
    let id_width = 4;

    // Case 1: Incomplete YAML - should parse with defaults
    {
        let queue_path = dir.path().join("incomplete.yaml");
        let raw = r#"version: 1
tasks:
  - id: RQ-0001
    status: todo
    title: Truncated task
    tags:
      - test
"#;
        std::fs::write(&queue_path, raw).context("write incomplete queue")?;

        let result = queue::load_queue(&queue_path);
        anyhow::ensure!(result.is_ok(), "incomplete YAML should parse with defaults");
        let queue = result?;
        anyhow::ensure!(queue.tasks.len() == 1, "should have 1 task");
        anyhow::ensure!(queue.tasks[0].id == "RQ-0001", "task ID should match");
    }

    // Case 2: Invalid types (string for version) - should fail
    {
        let queue_path = dir.path().join("invalid_types.yaml");
        let raw = r#"version: "one"
tasks:
  - id: RQ-0001
    status: todo
    title: Invalid type
    tags:
      - test
    scope:
      - crates/ralph
    evidence:
      - testing
    plan:
      - test
    request: test
    created_at: 2026-01-18T00:00:00Z
    updated_at: 2026-01-18T00:00:00Z
"#;
        std::fs::write(&queue_path, raw).context("write invalid types queue")?;

        let result = queue::load_queue(&queue_path);
        anyhow::ensure!(result.is_err(), "invalid version type should fail to parse");
    }

    // Case 3: Valid YAML should load successfully
    {
        let queue_path = dir.path().join("valid.yaml");
        let queue = QueueFile {
            version: 1,
            tasks: vec![make_task_with(1, TaskStatus::Todo, id_prefix, id_width)],
        };
        let raw = serde_yaml::to_string(&queue)?;
        std::fs::write(&queue_path, raw).context("write valid queue")?;

        let loaded = queue::load_queue(&queue_path).context("load valid YAML")?;
        anyhow::ensure!(loaded.tasks.len() == 1, "valid YAML should load");
    }

    Ok(())
}

#[test]
#[ignore]
fn stress_queue_ops_burn_in_long() -> Result<()> {
    if std::env::var("RALPH_STRESS_BURN_IN").ok().as_deref() != Some("1") {
        return Ok(());
    }
    // Burn-in: smaller queue, repeated archive + status updates + reload.
    // This is intentionally long-running and is executed by `make test` (which includes ignored tests).
    let dir = TempDir::new().context("create temp dir")?;
    let queue_path = dir.path().join("queue.json");
    let done_path = dir.path().join("done.json");

    let mut active = QueueFile {
        version: 1,
        tasks: Vec::new(),
    };
    let done = QueueFile {
        version: 1,
        tasks: Vec::new(),
    };

    // 2,000 tasks in active: first 200 start as done (eligible for archiving).
    for i in 1..=2000u32 {
        let status = if i <= 200 {
            TaskStatus::Done
        } else {
            TaskStatus::Todo
        };
        active.tasks.push(make_task(i, status));
    }

    queue::save_queue(&queue_path, &active).context("save initial active")?;
    queue::save_queue(&done_path, &done).context("save initial done")?;

    // Run a bounded number of iterations; each iteration archives done tasks and marks a few todo as done.
    for iter in 0..200u32 {
        let report = queue::archive_done_tasks(&queue_path, &done_path, ID_PREFIX, ID_WIDTH)
            .with_context(|| format!("archive iteration {iter}"))?;
        let _ = report;

        let mut current = queue::load_queue(&queue_path).context("load active")?;
        let now = "2026-01-18T00:00:00Z";

        // Mark a deterministic slice of todo tasks as done each iteration.
        let start = 201 + iter * 10;
        for id_num in start..start + 5 {
            let id = format!("RQ-{id_num:0width$}", width = ID_WIDTH);
            let _ = queue::set_status(&mut current, &id, TaskStatus::Done, now, None);
        }

        queue::save_queue(&queue_path, &current).context("save active")?;

        // Reload both and validate invariants.
        let active_reloaded = queue::load_queue(&queue_path).context("reload active")?;
        let done_reloaded = queue::load_queue(&done_path).context("reload done")?;
        queue::validate_queue_set(&active_reloaded, Some(&done_reloaded), ID_PREFIX, ID_WIDTH)
            .context("validate after iteration")?;
    }

    Ok(())
}
