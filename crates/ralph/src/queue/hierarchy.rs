//! Task hierarchy indexing and navigation based on `parent_id`.
//!
//! Responsibilities:
//! - Index tasks by parent_id for efficient child lookups.
//! - Provide safe traversal of parent/child relationships with cycle detection.
//! - Treat missing-parent tasks as "roots" and mark them as orphans during rendering.
//!
//! Not handled here:
//! - Queue persistence (see `crate::queue`).
//! - Validation logic (see `crate::queue::validation`).
//!
//! Invariants/assumptions:
//! - Task IDs are unique across active + done files.
//! - Ordering is deterministic: active tasks first (file order), then done tasks (file order).
//! - Cycles are detected and reported but do not cause infinite recursion.

use crate::contracts::{QueueFile, Task};
use std::collections::{HashMap, HashSet};

/// Source of a task in the combined active+done set.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TaskSource {
    Active,
    Done,
}

/// Reference to a task with metadata for ordering and source tracking.
#[derive(Clone, Copy, Debug)]
pub(crate) struct TaskRef<'a> {
    pub(crate) task: &'a Task,
    pub(crate) source: TaskSource,
    pub(crate) order: usize,
}

/// Index for efficient parent/child navigation.
#[derive(Debug)]
pub(crate) struct HierarchyIndex<'a> {
    by_id: HashMap<&'a str, TaskRef<'a>>,
    children_by_parent: HashMap<&'a str, Vec<TaskRef<'a>>>,
}

impl<'a> HierarchyIndex<'a> {
    /// Build a hierarchy index from active and optional done queues.
    ///
    /// Ordering: active tasks first (by file order), then done tasks (by file order).
    /// Children within each parent are stored in this deterministic order.
    pub(crate) fn build(active: &'a QueueFile, done: Option<&'a QueueFile>) -> Self {
        let mut by_id: HashMap<&'a str, TaskRef<'a>> = HashMap::new();
        let mut children_by_parent: HashMap<&'a str, Vec<TaskRef<'a>>> = HashMap::new();

        let mut order_counter: usize = 0;

        // Index active tasks first
        for task in &active.tasks {
            let id = task.id.trim();
            if id.is_empty() {
                continue;
            }
            let task_ref = TaskRef {
                task,
                source: TaskSource::Active,
                order: order_counter,
            };
            order_counter += 1;
            by_id.insert(id, task_ref);
        }

        // Index done tasks second
        if let Some(done_file) = done {
            for task in &done_file.tasks {
                let id = task.id.trim();
                if id.is_empty() {
                    continue;
                }
                let task_ref = TaskRef {
                    task,
                    source: TaskSource::Done,
                    order: order_counter,
                };
                order_counter += 1;
                by_id.insert(id, task_ref);
            }
        }

        // Build children map.
        for task_ref in by_id.values() {
            let task = task_ref.task;
            if let Some(parent_id) = task.parent_id.as_deref() {
                let parent_id_trimmed = parent_id.trim();
                if parent_id_trimmed.is_empty() {
                    // Treat empty/whitespace parent_id as unset
                    continue;
                }

                if by_id.contains_key(parent_id_trimmed) {
                    children_by_parent
                        .entry(parent_id_trimmed)
                        .or_default()
                        .push(*task_ref);
                }
            }
        }

        // Sort children within each parent by order for deterministic output
        for (_, children) in children_by_parent.iter_mut() {
            children.sort_by_key(|c| c.order);
        }

        Self {
            by_id,
            children_by_parent,
        }
    }

    /// Get a task by ID.
    pub(crate) fn get(&self, id: &str) -> Option<TaskRef<'a>> {
        self.by_id.get(id.trim()).copied()
    }

    /// Get children of a specific parent task.
    pub(crate) fn children_of(&self, parent_id: &str) -> &[TaskRef<'a>] {
        self.children_by_parent
            .get(parent_id.trim())
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    /// Check if a task ID exists in the index.
    pub(crate) fn contains(&self, id: &str) -> bool {
        self.by_id.contains_key(id.trim())
    }

    /// Get all root tasks (tasks with no parent or with orphaned parent references).
    /// Roots are returned in deterministic order (by their order field).
    pub(crate) fn roots(&self) -> Vec<TaskRef<'a>> {
        let mut roots: Vec<TaskRef<'a>> = self
            .by_id
            .values()
            .filter(|task_ref| {
                let parent_id = task_ref.task.parent_id.as_deref();
                match parent_id {
                    None => true,
                    Some(pid) => {
                        let pid_trimmed = pid.trim();
                        pid_trimmed.is_empty() || !self.by_id.contains_key(pid_trimmed)
                    }
                }
            })
            .copied()
            .collect();

        roots.sort_by_key(|r| r.order);
        roots
    }
}

/// Detect cycles in the parent_id chain.
/// Returns a list of task IDs involved in cycles.
pub(crate) fn detect_parent_cycles(all_tasks: &[&Task]) -> Vec<Vec<String>> {
    let mut child_to_parent: HashMap<&str, &str> = HashMap::new();

    // Build parent pointer map
    for task in all_tasks {
        let task_id = task.id.trim();
        if task_id.is_empty() {
            continue;
        }
        if let Some(parent_id) = task.parent_id.as_deref() {
            let parent_id_trimmed = parent_id.trim();
            if !parent_id_trimmed.is_empty() {
                child_to_parent.insert(task_id, parent_id_trimmed);
            }
        }
    }

    let mut visited: HashSet<&str> = HashSet::new();
    let mut cycles: Vec<Vec<String>> = Vec::new();

    for &start_task in all_tasks {
        let start_id = start_task.id.trim();
        if start_id.is_empty() || visited.contains(start_id) {
            continue;
        }

        let mut path: Vec<&str> = Vec::new();
        let mut current = Some(start_id);
        let mut path_set: HashSet<&str> = HashSet::new();

        while let Some(node) = current {
            if path_set.contains(node) {
                // Cycle detected - extract cycle from path
                let cycle_start = path.iter().position(|&x| x == node).unwrap_or(0);
                let cycle: Vec<String> =
                    path[cycle_start..].iter().map(|&s| s.to_string()).collect();

                // Only add if we haven't seen this exact cycle
                let cycle_normalized = normalize_cycle(&cycle);
                if !cycles
                    .iter()
                    .any(|c| normalize_cycle(c) == cycle_normalized)
                {
                    cycles.push(cycle);
                }
                break;
            }

            if visited.contains(node) {
                // We've hit a previously processed path, no cycle here
                break;
            }

            path.push(node);
            path_set.insert(node);
            current = child_to_parent.get(node).copied();
        }

        // Mark all nodes in this path as visited
        for &node in &path {
            visited.insert(node);
        }
    }

    cycles
}

/// Normalize a cycle for deduplication (rotates to start with minimum element).
fn normalize_cycle(cycle: &[String]) -> Vec<String> {
    if cycle.is_empty() {
        return cycle.to_vec();
    }

    let min_idx = cycle
        .iter()
        .enumerate()
        .min_by_key(|(_, v)| *v)
        .map(|(i, _)| i)
        .unwrap_or(0);

    let mut normalized: Vec<String> = cycle[min_idx..].to_vec();
    normalized.extend_from_slice(&cycle[..min_idx]);
    normalized
}

/// Render a task hierarchy tree as ASCII art.
///
/// Arguments:
/// - `idx`: The hierarchy index
/// - `roots`: Root task IDs to start rendering from
/// - `max_depth`: Maximum depth to render
/// - `include_done`: Whether to include done tasks in output
/// - `format_line`: Closure to format a task line
pub(crate) fn render_tree<F>(
    idx: &HierarchyIndex<'_>,
    roots: &[&str],
    max_depth: usize,
    include_done: bool,
    format_line: F,
) -> String
where
    F: Fn(&Task, usize, bool, Option<&str>) -> String,
{
    let mut output = String::new();
    let mut visited: HashSet<String> = HashSet::new();

    struct RenderCtx<'idx, 'task, 'out, F>
    where
        F: Fn(&Task, usize, bool, Option<&str>) -> String,
    {
        idx: &'idx HierarchyIndex<'task>,
        max_depth: usize,
        include_done: bool,
        visited: &'out mut HashSet<String>,
        output: &'out mut String,
        format_line: &'out F,
    }

    fn render_tree_recursive<'idx, 'task, 'out, F>(
        ctx: &mut RenderCtx<'idx, 'task, 'out, F>,
        task_id: &str,
        depth: usize,
    ) where
        F: Fn(&Task, usize, bool, Option<&str>) -> String,
    {
        if depth > ctx.max_depth {
            return;
        }

        let trimmed_id = task_id.trim();
        if ctx.visited.contains(trimmed_id) {
            // Cycle detected - mark and stop descending
            if let Some(task_ref) = ctx.idx.get(trimmed_id) {
                let line = (ctx.format_line)(task_ref.task, depth, true, None);
                ctx.output.push_str(&line);
                ctx.output.push('\n');
            }
            return;
        }

        let task_ref = match ctx.idx.get(trimmed_id) {
            Some(t) => t,
            None => {
                // Missing reference - shouldn't happen if roots are computed correctly
                return;
            }
        };

        // Skip done tasks unless include_done is true
        if !ctx.include_done && matches!(task_ref.source, TaskSource::Done) {
            return;
        }

        ctx.visited.insert(trimmed_id.to_string());

        // Check if this is an orphan (has parent_id but parent not in index)
        let is_orphan = task_ref
            .task
            .parent_id
            .as_deref()
            .map(|p| !p.trim().is_empty() && !ctx.idx.contains(p))
            .unwrap_or(false);

        let orphan_parent = if is_orphan {
            task_ref.task.parent_id.as_deref()
        } else {
            None
        };

        let line = (ctx.format_line)(task_ref.task, depth, false, orphan_parent);
        ctx.output.push_str(&line);
        ctx.output.push('\n');

        // Recurse into children (sorted by order via HierarchyIndex)
        let children = ctx.idx.children_of(trimmed_id);
        for child in children {
            render_tree_recursive(ctx, &child.task.id, depth + 1);
        }
    }

    let mut ctx = RenderCtx {
        idx,
        max_depth,
        include_done,
        visited: &mut visited,
        output: &mut output,
        format_line: &format_line,
    };

    for &root_id in roots {
        render_tree_recursive(&mut ctx, root_id, 0);
    }

    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contracts::{Task, TaskStatus};

    fn make_task(id: &str, parent_id: Option<&str>) -> Task {
        Task {
            id: id.to_string(),
            title: format!("Task {}", id),
            status: TaskStatus::Todo,
            parent_id: parent_id.map(|s| s.to_string()),
            created_at: Some("2026-01-01T00:00:00Z".to_string()),
            updated_at: Some("2026-01-01T00:00:00Z".to_string()),
            ..Default::default()
        }
    }

    fn make_active_queue(tasks: Vec<Task>) -> QueueFile {
        QueueFile { version: 1, tasks }
    }

    #[test]
    fn hierarchy_index_builds_correctly() {
        let tasks = vec![
            make_task("RQ-0001", None),
            make_task("RQ-0002", Some("RQ-0001")),
            make_task("RQ-0003", Some("RQ-0001")),
        ];
        let active = make_active_queue(tasks);

        let idx = HierarchyIndex::build(&active, None);

        assert_eq!(idx.children_of("RQ-0001").len(), 2);
        assert!(idx.children_of("RQ-0002").is_empty());
        assert!(idx.get("RQ-0001").is_some());
        assert!(idx.get("RQ-0002").is_some());
    }

    #[test]
    fn children_preserve_file_order() {
        let tasks = vec![
            make_task("RQ-0001", None),
            make_task("RQ-0003", Some("RQ-0001")), // Note: 0003 before 0002
            make_task("RQ-0002", Some("RQ-0001")),
        ];
        let active = make_active_queue(tasks);

        let idx = HierarchyIndex::build(&active, None);
        let children = idx.children_of("RQ-0001");

        assert_eq!(children[0].task.id, "RQ-0003");
        assert_eq!(children[1].task.id, "RQ-0002");
    }

    #[test]
    fn orphan_detection_works() {
        let tasks = vec![
            make_task("RQ-0001", None),
            make_task("RQ-0002", Some("RQ-9999")), // Missing parent
        ];
        let active = make_active_queue(tasks);

        let idx = HierarchyIndex::build(&active, None);

        // Orphans are treated as roots for rendering/navigation.
        let roots: Vec<&str> = idx.roots().iter().map(|r| r.task.id.as_str()).collect();
        assert!(roots.contains(&"RQ-0002"));

        let output = render_tree(
            &idx,
            &["RQ-0002"],
            10,
            true,
            |_task, _depth, _is_cycle, orphan_parent| {
                format!("orphan_parent={}", orphan_parent.unwrap_or("<none>"))
            },
        );
        assert!(output.contains("orphan_parent=RQ-9999"));
    }

    #[test]
    fn empty_parent_id_treated_as_unset() {
        let mut task = make_task("RQ-0001", None);
        task.parent_id = Some("   ".to_string()); // Whitespace only

        let active = make_active_queue(vec![task]);
        let idx = HierarchyIndex::build(&active, None);

        assert_eq!(idx.roots().len(), 1);
    }

    #[test]
    fn cycle_detection_finds_simple_cycle() {
        let tasks = [
            Task {
                id: "RQ-0001".to_string(),
                parent_id: Some("RQ-0002".to_string()),
                title: "Task 1".to_string(),
                created_at: Some("2026-01-01T00:00:00Z".to_string()),
                updated_at: Some("2026-01-01T00:00:00Z".to_string()),
                ..Default::default()
            },
            Task {
                id: "RQ-0002".to_string(),
                parent_id: Some("RQ-0001".to_string()),
                title: "Task 2".to_string(),
                created_at: Some("2026-01-01T00:00:00Z".to_string()),
                updated_at: Some("2026-01-01T00:00:00Z".to_string()),
                ..Default::default()
            },
        ];

        let task_refs: Vec<&Task> = tasks.iter().collect();
        let cycles = detect_parent_cycles(&task_refs);

        assert_eq!(cycles.len(), 1);
        assert_eq!(cycles[0].len(), 2);
    }

    #[test]
    fn cycle_detection_finds_self_cycle() {
        let tasks = [Task {
            id: "RQ-0001".to_string(),
            parent_id: Some("RQ-0001".to_string()),
            title: "Task 1".to_string(),
            created_at: Some("2026-01-01T00:00:00Z".to_string()),
            updated_at: Some("2026-01-01T00:00:00Z".to_string()),
            ..Default::default()
        }];

        let task_refs: Vec<&Task> = tasks.iter().collect();
        let cycles = detect_parent_cycles(&task_refs);

        assert_eq!(cycles.len(), 1);
        assert_eq!(cycles[0], vec!["RQ-0001"]);
    }

    #[test]
    fn roots_includes_orphans() {
        let tasks = vec![
            make_task("RQ-0001", None),
            make_task("RQ-0002", Some("RQ-9999")), // Orphan
        ];
        let active = make_active_queue(tasks);

        let idx = HierarchyIndex::build(&active, None);
        let roots = idx.roots();

        assert_eq!(roots.len(), 2);
    }

    #[test]
    fn active_done_combined_ordering() {
        let active = make_active_queue(vec![make_task("RQ-0001", None)]);
        let done = QueueFile {
            version: 1,
            tasks: vec![make_task("RQ-0002", Some("RQ-0001"))],
        };

        let idx = HierarchyIndex::build(&active, Some(&done));

        assert!(idx.get("RQ-0001").is_some());
        assert!(idx.get("RQ-0002").is_some());

        // RQ-0001 should have lower order than RQ-0002
        let r1 = idx.get("RQ-0001").unwrap();
        let r2 = idx.get("RQ-0002").unwrap();
        assert!(r1.order < r2.order);
    }

    #[test]
    fn tree_rendering_produces_output() {
        let tasks = vec![
            make_task("RQ-0001", None),
            make_task("RQ-0002", Some("RQ-0001")),
        ];
        let active = make_active_queue(tasks);
        let idx = HierarchyIndex::build(&active, None);

        let output = render_tree(
            &idx,
            &["RQ-0001"],
            10,
            true,
            |task, depth, _is_cycle, _orphan| format!("{}{}", "  ".repeat(depth), task.id),
        );

        assert!(output.contains("RQ-0001"));
        assert!(output.contains("  RQ-0002"));
    }

    #[test]
    fn tree_rendering_respects_max_depth() {
        let tasks = vec![
            make_task("RQ-0001", None),
            make_task("RQ-0002", Some("RQ-0001")),
            make_task("RQ-0003", Some("RQ-0002")),
        ];
        let active = make_active_queue(tasks);
        let idx = HierarchyIndex::build(&active, None);

        let output = render_tree(
            &idx,
            &["RQ-0001"],
            1,
            true,
            |task, depth, _is_cycle, _orphan| format!("{}{}", "  ".repeat(depth), task.id),
        );

        assert!(output.contains("RQ-0001"));
        assert!(output.contains("RQ-0002"));
        assert!(!output.contains("RQ-0003"));
    }

    #[test]
    fn tree_rendering_handles_cycles() {
        let tasks = vec![
            Task {
                id: "RQ-0001".to_string(),
                parent_id: Some("RQ-0002".to_string()),
                title: "Task 1".to_string(),
                created_at: Some("2026-01-01T00:00:00Z".to_string()),
                updated_at: Some("2026-01-01T00:00:00Z".to_string()),
                ..Default::default()
            },
            Task {
                id: "RQ-0002".to_string(),
                parent_id: Some("RQ-0001".to_string()),
                title: "Task 2".to_string(),
                created_at: Some("2026-01-01T00:00:00Z".to_string()),
                updated_at: Some("2026-01-01T00:00:00Z".to_string()),
                ..Default::default()
            },
        ];
        let active = make_active_queue(tasks);
        let idx = HierarchyIndex::build(&active, None);

        let output = render_tree(
            &idx,
            &["RQ-0001"],
            10,
            true,
            |task, depth, is_cycle, _orphan| {
                let marker = if is_cycle { " (cycle)" } else { "" };
                format!("{}{}{}", "  ".repeat(depth), task.id, marker)
            },
        );

        assert!(output.contains("RQ-0001"));
        assert!(output.contains("(cycle)"));
    }
}
