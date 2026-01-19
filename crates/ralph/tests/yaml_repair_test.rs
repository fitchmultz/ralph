use ralph::queue;
use std::fs;
use tempfile::TempDir;

#[test]
fn repair_handles_nested_colons_in_list_items() -> anyhow::Result<()> {
    let dir = TempDir::new()?;
    let path = dir.path().join("queue.yaml");
    let broken = r#"
version: 1
tasks:
  - id: RQ-0001
    status: todo
    title: Fix bug
    tags: [rust]
    scope: [crates]
    evidence:
      - error: invalid type
      - nested: colon: value
    plan: []
    notes: []
    request: fix it
    created_at: 2026-01-18T00:00:00Z
    updated_at: 2026-01-18T00:00:00Z
"#;
    fs::write(&path, broken)?;

    let report = queue::repair_queue(&path)?;
    assert!(report.repaired);

    let (queue, _) = queue::load_queue_with_repair(&path)?;
    assert_eq!(queue.tasks.len(), 1);
    assert_eq!(queue.tasks[0].evidence.len(), 2);
    assert_eq!(queue.tasks[0].evidence[0], "error: invalid type");
    assert_eq!(queue.tasks[0].evidence[1], "nested: colon: value");

    Ok(())
}

#[test]
fn repair_handles_colons_in_mapping_values() -> anyhow::Result<()> {
    let dir = TempDir::new()?;
    let path = dir.path().join("queue.yaml");
    let broken = r#"
version: 1
tasks:
  - id: RQ-0001
    status: todo
    title: Fix: the title
    tags: []
    scope: []
    evidence: []
    plan:
      - step 1: do this
    notes:
      - note: value
    request: req: value
    created_at: 2026-01-18T00:00:00Z
    updated_at: 2026-01-18T00:00:00Z
"#;
    fs::write(&path, broken)?;

    let report = queue::repair_queue(&path)?;
    assert!(report.repaired);

    let (queue, _) = queue::load_queue_with_repair(&path)?;
    assert_eq!(queue.tasks[0].title, "Fix: the title");
    assert_eq!(queue.tasks[0].plan[0], "step 1: do this");
    assert_eq!(queue.tasks[0].notes[0], "note: value");
    assert_eq!(queue.tasks[0].request.as_deref(), Some("req: value"));

    Ok(())
}

#[test]
fn repair_handles_comments() -> anyhow::Result<()> {
    let dir = TempDir::new()?;
    let path = dir.path().join("queue.yaml");
    let broken = r#"
version: 1
tasks:
  - id: RQ-0001
    status: todo
    title: Title # comment
    tags: []
    scope: []
    evidence:
      - evidence # comment
    plan: []
    notes: []
    request: req
    created_at: 2026-01-18T00:00:00Z
    updated_at: 2026-01-18T00:00:00Z
"#;
    fs::write(&path, broken)?;

    let (queue, _) = queue::load_queue_with_repair(&path)?;
    // serde_yaml handles comments, so no repair needed and title should be just "Title" (if serde handles it correctly)
    // Actually, "Title # comment" unquoted in YAML:
    // If # is preceded by space, it is a comment.
    // So title is "Title".
    assert_eq!(queue.tasks[0].title, "Title");

    Ok(())
}

#[test]
fn repair_handles_truncated_yaml_structure() -> anyhow::Result<()> {
    let dir = TempDir::new()?;
    let path = dir.path().join("queue.yaml");
    // Indentation error
    let really_broken = r#"
version: 1
  tasks:
  - id: RQ-0001
    status: todo
    title: T
    tags: []
    scope: []
    evidence: []
    plan: []
    notes: []
    request: r
    created_at: 2026-01-18T00:00:00Z
    updated_at: 2026-01-18T00:00:00Z
"#;
    fs::write(&path, really_broken)?;

    let report = queue::repair_queue(&path)?;
    assert!(report.repaired);

    let raw = fs::read_to_string(&path)?;
    assert!(raw.contains("tasks:\n"));

    Ok(())
}

#[test]
fn repair_preserves_nested_objects_structure() -> anyhow::Result<()> {
    let dir = TempDir::new()?;
    let path = dir.path().join("queue.yaml");
    // Indented tasks (4 spaces) - valid YAML but triggers indent > 2 logic in repair
    // We introduce a colon error in 'title' to force repair execution.
    let broken_and_indented = r#"
version: 1
tasks:
    - id: RQ-0001
      status: todo
      title: Broken: title
      tags: []
      scope: []
      evidence: []
      plan: []
      notes: []
      request: r
      created_at: 2026-01-18T00:00:00Z
      updated_at: 2026-01-18T00:00:00Z
"#;
    fs::write(&path, broken_and_indented)?;

    let report = queue::repair_queue(&path)?;
    assert!(report.repaired);

    let (queue, _) = queue::load_queue_with_repair(&path)?;
    assert_eq!(queue.tasks[0].title, "Broken: title");
    assert_eq!(queue.tasks[0].id, "RQ-0001");

    Ok(())
}
