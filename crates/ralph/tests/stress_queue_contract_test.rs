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

fn build_raw_yaml_with_colons(task_count: u32, id_prefix: &str, id_width: usize) -> String {
    let mut out = String::from("version: 1\ntasks:\n");
    for i in 1..=task_count {
        out.push_str(&format!(
            "  - id: {id_prefix}-{i:0width$}\n",
            width = id_width
        ));
        out.push_str("    status: todo\n");
        out.push_str(&format!("    title: Task {i}: needs repair\n"));
        out.push_str("    tags:\n      - rust\n");
        out.push_str("    scope:\n      - crates/ralph\n");
        out.push_str(&format!(
            "    evidence:\n      - evidence {i}: contains colon\n"
        ));
        out.push_str(&format!("    plan:\n      - plan {i}: exercise repair\n"));
        out.push_str("    notes: []\n");
        out.push_str("    request: stress test\n");
        out.push_str("    created_at: 2026-01-18T00:00:00Z\n");
        out.push_str("    updated_at: 2026-01-18T00:00:00Z\n");
    }
    out
}

fn make_task(id_num: u32, status: TaskStatus) -> Task {
    make_task_with(id_num, status, ID_PREFIX, ID_WIDTH)
}

fn write_queue_files(
    dir: &TempDir,
    active: &QueueFile,
    done: &QueueFile,
) -> Result<(PathBuf, PathBuf)> {
    let queue_path = dir.path().join("queue.yaml");
    let done_path = dir.path().join("done.yaml");
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

    let (reloaded_active, repaired_active) =
        queue::load_queue_with_repair(&queue_path, ID_PREFIX, ID_WIDTH).context("load active")?;
    anyhow::ensure!(!repaired_active, "unexpected repair on valid YAML (active)");

    let (reloaded_done, repaired_done) =
        queue::load_queue_with_repair(&done_path, ID_PREFIX, ID_WIDTH).context("load done")?;
    anyhow::ensure!(!repaired_done, "unexpected repair on valid YAML (done)");

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
        let (mut current, repaired_current) =
            queue::load_queue_with_repair(&queue_path, ID_PREFIX, ID_WIDTH)
                .context("load active")?;
        anyhow::ensure!(
            !repaired_current,
            "unexpected repair on valid YAML (active)"
        );
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

        let active_reloaded = queue::load_queue_with_repair(&queue_path, ID_PREFIX, ID_WIDTH)
            .context("reload active")?;
        anyhow::ensure!(
            !active_reloaded.1,
            "unexpected repair on valid YAML (active)"
        );
        let done_reloaded = queue::load_queue_with_repair(&done_path, ID_PREFIX, ID_WIDTH)
            .context("reload done")?;
        anyhow::ensure!(!done_reloaded.1, "unexpected repair on valid YAML (done)");
        queue::validate_queue_set(
            &active_reloaded.0,
            Some(&done_reloaded.0),
            ID_PREFIX,
            ID_WIDTH,
        )
        .context("validate after iteration")?;
    }

    Ok(())
}

#[test]
fn stress_queue_repair_large_yaml_scalars() -> Result<()> {
    let dir = TempDir::new().context("create temp dir")?;
    let queue_path = dir.path().join("queue.yaml");
    let raw = build_raw_yaml_with_colons(2000, ID_PREFIX, ID_WIDTH);
    std::fs::write(&queue_path, raw).context("write raw queue")?;

    let (queue, repaired) = queue::load_queue_with_repair(&queue_path, ID_PREFIX, ID_WIDTH)?;
    anyhow::ensure!(repaired, "expected repair for colon scalars");
    anyhow::ensure!(queue.tasks.len() == 2000, "unexpected task count");

    queue::validate_queue(&queue, ID_PREFIX, ID_WIDTH).context("validate repaired queue")?;

    Ok(())
}

#[test]
fn stress_queue_repair_edge_cases() -> Result<()> {
    let dir = TempDir::new().context("create temp dir")?;
    let id_prefix = "RQ";
    let id_width = 4;

    // Case 1: Truncated YAML
    {
        let queue_path = dir.path().join("truncated.yaml");
        let raw = r#"version: 1
tasks:
  - id: RQ-0001
    status: todo
    title: Truncated task
    tags:
      - test
"#;
        std::fs::write(&queue_path, raw).context("write truncated queue")?;

        // This should fail to parse but might be repairable if repair_queue_schema can handle partials.
        // Actually, repair_queue_schema uses serde_yaml::from_str(raw).ok()?, which will return None for invalid YAML.
        // So it depends on if the other repair_yaml_* functions can make it valid enough for serde_yaml.
        let result = queue::load_queue_with_repair(&queue_path, id_prefix, id_width);
        // Truncated YAML usually cannot be safely repaired unless we know how to close it.
        // Ralph's repair logic currently doesn't "close" truncated YAML, so this might remain an error.
        // But we want to see HOW it fails or if it can recover anything.
        if let Ok((q, repaired)) = result {
            println!(
                "Truncated YAML repaired: {}, tasks: {}",
                repaired,
                q.tasks.len()
            );
        } else {
            println!("Truncated YAML correctly failed to parse/repair");
        }
    }

    // Case 2: Invalid types (string for version)
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

        let (queue, repaired) = queue::load_queue_with_repair(&queue_path, id_prefix, id_width)
            .context("load invalid types")?;
        anyhow::ensure!(repaired, "expected repair for version type mismatch");
        anyhow::ensure!(queue.version == 1, "version should be repaired to 1");
    }

    // Case 3: Nested scalar colons
    {
        let queue_path = dir.path().join("nested_colons.yaml");
        let raw = r#"version: 1
tasks:
  - id: RQ-0001
    status: todo
    title: Task with : nested : colons
    tags:
      - test:tag
    scope:
      - crates/ralph:src/queue.rs
    evidence:
      - evidence: with : multiple : colons
    plan:
      - plan: with:colons
    request: test
    created_at: 2026-01-18T00:00:00Z
    updated_at: 2026-01-18T00:00:00Z
"#;
        std::fs::write(&queue_path, raw).context("write nested colons queue")?;

        let (queue, repaired) = queue::load_queue_with_repair(&queue_path, id_prefix, id_width)
            .context("load nested colons")?;
        anyhow::ensure!(repaired, "expected repair for nested colons");
        anyhow::ensure!(queue.tasks[0].title == "Task with : nested : colons");
        anyhow::ensure!(queue.tasks[0].evidence[0] == "evidence: with : multiple : colons");
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
    let queue_path = dir.path().join("queue.yaml");
    let done_path = dir.path().join("done.yaml");

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

        let (mut current, repaired_current) =
            queue::load_queue_with_repair(&queue_path, ID_PREFIX, ID_WIDTH)
                .context("load active")?;
        anyhow::ensure!(
            !repaired_current,
            "unexpected repair on valid YAML (active)"
        );
        let now = "2026-01-18T00:00:00Z";

        // Mark a deterministic slice of todo tasks as done each iteration.
        let start = 201 + iter * 10;
        for id_num in start..start + 5 {
            let id = format!("RQ-{id_num:0width$}", width = ID_WIDTH);
            let _ = queue::set_status(&mut current, &id, TaskStatus::Done, now, None);
        }

        queue::save_queue(&queue_path, &current).context("save active")?;

        // Reload both and validate invariants.
        let active_reloaded = queue::load_queue_with_repair(&queue_path, ID_PREFIX, ID_WIDTH)
            .context("reload active")?;
        anyhow::ensure!(
            !active_reloaded.1,
            "unexpected repair on valid YAML (active)"
        );
        let done_reloaded = queue::load_queue_with_repair(&done_path, ID_PREFIX, ID_WIDTH)
            .context("reload done")?;
        anyhow::ensure!(!done_reloaded.1, "unexpected repair on valid YAML (done)");
        queue::validate_queue_set(
            &active_reloaded.0,
            Some(&done_reloaded.0),
            ID_PREFIX,
            ID_WIDTH,
        )
        .context("validate after iteration")?;
    }

    Ok(())
}
