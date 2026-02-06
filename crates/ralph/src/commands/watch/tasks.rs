//! Task handling for the watch command.
//!
//! Responsibilities:
//! - Handle detected comments by creating tasks or suggesting them.
//! - Check for duplicate tasks (deduplication).
//! - Create tasks from detected comments.
//! - Generate unique task IDs using shared queue helpers.
//!
//! Not handled here:
//! - Comment detection (see `comments.rs`).
//! - File watching (see `event_loop.rs`).
//!
//! Invariants/assumptions:
//! - Task IDs are generated using `queue::next_id_across` for correctness.
//! - Deduplication uses fingerprint-based matching for watch tasks with legacy fallback.
//! - Queue is loaded and saved atomically within this module.

use crate::commands::watch::types::{DetectedComment, WatchOptions};
use crate::config::Resolved;
use crate::contracts::{QueueFile, Task, TaskPriority, TaskStatus};
use crate::notification::{NotificationConfig, notify_watch_new_task};
use crate::queue::{load_queue, load_queue_or_default, save_queue, suggest_new_task_insert_index};
use crate::timeutil;
use anyhow::Result;
use sha2::{Digest, Sha256};
use std::collections::HashMap;

/// Handle detected comments by creating tasks or suggesting them.
pub fn handle_detected_comments(
    resolved: &Resolved,
    comments: &[DetectedComment],
    opts: &WatchOptions,
) -> Result<()> {
    // Load current queue
    let mut queue = load_queue(&resolved.queue_path)?;

    // Track which tasks were created
    let mut created_tasks: Vec<(String, String)> = Vec::new();

    for comment in comments {
        // Check if a similar task already exists
        if task_exists_for_comment(&queue, comment) {
            log::debug!(
                "Skipping duplicate task for {}:{}",
                comment.file_path.display(),
                comment.line_number
            );
            continue;
        }

        let task = create_task_from_comment(comment, resolved)?;

        if opts.auto_queue {
            // Add task to queue
            let insert_at = suggest_new_task_insert_index(&queue);
            queue.tasks.insert(insert_at, task.clone());
            created_tasks.push((task.id.clone(), task.title.clone()));
            log::info!("Created task {}: {}", task.id, task.title);
        } else {
            // Just log the suggestion
            let type_str = format!("{:?}", comment.comment_type).to_uppercase();
            log::info!(
                "[SUGGESTION] {} at {}:{}",
                type_str,
                comment.file_path.display(),
                comment.line_number
            );
            log::info!("  Content: {}", comment.content);
            log::info!("  Suggested task: {}", task.title);
        }
    }

    // Save queue if tasks were created
    if opts.auto_queue && !created_tasks.is_empty() {
        save_queue(&resolved.queue_path, &queue)?;
        log::info!("Added {} task(s) to queue", created_tasks.len());

        // Send notification if enabled
        if opts.notify {
            let config = NotificationConfig::new();
            notify_watch_new_task(created_tasks.len(), &config);
        }
    }

    // After creating tasks from comments, reconcile existing watch tasks
    if opts.close_removed {
        let closed = reconcile_watch_tasks(resolved, comments, opts)?;
        if !closed.is_empty() {
            log::info!(
                "Reconciled {} watch task(s) due to removed comments",
                closed.len()
            );
        }
    }

    Ok(())
}

/// Reconcile watch tasks against currently detected comments.
/// Marks watch tasks as done when their originating comments are no longer present.
pub fn reconcile_watch_tasks(
    resolved: &Resolved,
    detected_comments: &[DetectedComment],
    _opts: &WatchOptions,
) -> Result<Vec<String>> {
    let mut queue = load_queue(&resolved.queue_path)?;
    let mut closed_tasks: Vec<String> = Vec::new();
    let now = crate::timeutil::now_utc_rfc3339_or_fallback();

    // Build set of current fingerprints and file+line combos
    let current_fingerprints: std::collections::HashSet<String> = detected_comments
        .iter()
        .map(|c| generate_comment_fingerprint(&c.content))
        .collect();

    let current_locations: std::collections::HashSet<(String, usize)> = detected_comments
        .iter()
        .map(|c| (c.file_path.to_string_lossy().to_string(), c.line_number))
        .collect();

    for task in &mut queue.tasks {
        // Only process watch-created tasks that are still active
        if !task.tags.contains(&"watch".to_string()) {
            continue;
        }
        if task.status == TaskStatus::Done || task.status == TaskStatus::Rejected {
            continue;
        }

        // Check if this task's comment still exists
        let comment_still_exists =
            if let Some(task_fingerprint) = task.custom_fields.get("watch.fingerprint") {
                // Fingerprint-based check (preferred)
                current_fingerprints.contains(task_fingerprint)
            } else if let (Some(task_file), Some(task_line)) = (
                task.custom_fields.get("watch.file"),
                task.custom_fields
                    .get("watch.line")
                    .and_then(|l| l.parse::<usize>().ok()),
            ) {
                // Location-based fallback
                current_locations.contains(&(task_file.clone(), task_line))
            } else {
                // Legacy task without structured metadata - skip reconciliation
                // (can't reliably determine if comment was removed)
                true
            };

        if !comment_still_exists {
            // Comment removed - mark task as done and add note
            task.status = TaskStatus::Done;
            task.completed_at = Some(now.clone());
            task.notes.push(format!(
                "[watch] Automatically marked done: originating comment was removed from source file at {}",
                now
            ));
            task.updated_at = Some(now.clone());
            closed_tasks.push(task.id.clone());
            log::info!("Closed task {}: originating comment was removed", task.id);
        }
    }

    // Save queue if any tasks were closed
    if !closed_tasks.is_empty() {
        save_queue(&resolved.queue_path, &queue)?;
        log::info!(
            "Closed {} task(s) due to comment removal",
            closed_tasks.len()
        );
    }

    Ok(closed_tasks)
}

/// Generate a stable fingerprint for comment content.
/// Normalizes content (lowercase, trim whitespace) and returns SHA256 prefix.
pub fn generate_comment_fingerprint(content: &str) -> String {
    let normalized = content.to_lowercase().trim().to_string();
    let mut hasher = Sha256::new();
    hasher.update(normalized.as_bytes());
    let result = hasher.finalize();
    // Use first 16 chars of hex for readability while maintaining uniqueness
    format!("{:x}", result)[..16].to_string()
}

/// Check if a task already exists for a given comment.
/// Uses fingerprint-based matching for watch-created tasks, falling back to
/// file/line matching for backward compatibility.
pub fn task_exists_for_comment(queue: &QueueFile, comment: &DetectedComment) -> bool {
    let fingerprint = generate_comment_fingerprint(&comment.content);
    let file_str = comment.file_path.to_string_lossy().to_string();

    queue.tasks.iter().any(|task| {
        // First check: Is this a watch-created task with structured metadata?
        if task.tags.contains(&"watch".to_string()) {
            // Check fingerprint match (strongest signal)
            if let Some(task_fingerprint) = task.custom_fields.get("watch.fingerprint")
                && task_fingerprint == &fingerprint
            {
                return true;
            }

            // Fallback: Check file and line match for watch tasks without fingerprint
            if let (Some(task_file), Some(task_line)) = (
                task.custom_fields.get("watch.file"),
                task.custom_fields.get("watch.line"),
            ) && task_file == &file_str
                && task_line == &comment.line_number.to_string()
            {
                return true;
            }
        }

        // Legacy fallback: Check if task title or notes reference this file and line
        // This handles user-created tasks and watch tasks from older versions
        let title_match = task.title.contains(&file_str)
            || task
                .title
                .contains(&format!("line {}", comment.line_number));

        let notes_match = task.notes.iter().any(|note| {
            note.contains(&file_str) && note.contains(&format!("{}", comment.line_number))
        });

        title_match || notes_match
    })
}

/// Create a task from a detected comment.
pub fn create_task_from_comment(comment: &DetectedComment, resolved: &Resolved) -> Result<Task> {
    let type_str = format!("{:?}", comment.comment_type).to_uppercase();
    let file_name = comment
        .file_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown");

    let title = format!(
        "{}: {} in {}",
        type_str,
        comment.content.chars().take(50).collect::<String>(),
        file_name
    );

    let now = timeutil::now_utc_rfc3339_or_fallback();

    // Generate a unique task ID using the shared queue helper
    let task_id = generate_task_id(resolved)?;

    // Generate fingerprint for deduplication
    let fingerprint = generate_comment_fingerprint(&comment.content);

    // Populate custom_fields with structured watch metadata
    let mut custom_fields = HashMap::new();
    custom_fields.insert(
        "watch.file".to_string(),
        comment.file_path.to_string_lossy().to_string(),
    );
    custom_fields.insert("watch.line".to_string(), comment.line_number.to_string());
    custom_fields.insert(
        "watch.comment_type".to_string(),
        format!("{:?}", comment.comment_type).to_lowercase(),
    );
    custom_fields.insert("watch.fingerprint".to_string(), fingerprint);
    custom_fields.insert("watch.version".to_string(), "1".to_string());

    let notes = vec![
        format!(
            "Detected in: {}:{}",
            comment.file_path.display(),
            comment.line_number
        ),
        format!("Full content: {}", comment.content),
        format!("Context: {}", comment.context),
    ];

    let tags = vec![
        "watch".to_string(),
        format!("{:?}", comment.comment_type).to_lowercase(),
    ];

    Ok(Task {
        id: task_id,
        status: TaskStatus::Todo,
        title,
        description: None,
        priority: TaskPriority::Medium,
        tags,
        scope: vec![comment.file_path.to_string_lossy().to_string()],
        evidence: Vec::new(),
        plan: Vec::new(),
        notes,
        request: Some(format!("Address {} comment", type_str)),
        agent: None,
        created_at: Some(now.clone()),
        updated_at: Some(now),
        completed_at: None,
        started_at: None,
        scheduled_start: None,
        depends_on: Vec::new(),
        blocks: Vec::new(),
        relates_to: Vec::new(),
        duplicates: None,
        custom_fields,
        parent_id: None,
    })
}

/// Generate a unique task ID using the shared queue helper.
fn generate_task_id(resolved: &Resolved) -> Result<String> {
    let active_queue = load_queue_or_default(&resolved.queue_path)?;
    let done_queue = if resolved.done_path.exists() {
        Some(load_queue_or_default(&resolved.done_path)?)
    } else {
        None
    };

    let max_depth = resolved.config.queue.max_dependency_depth.unwrap_or(10);

    crate::queue::next_id_across(
        &active_queue,
        done_queue.as_ref(),
        &resolved.id_prefix,
        resolved.id_width,
        max_depth,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::watch::types::CommentType;
    use crate::contracts::{Config, QueueFile};
    use std::path::PathBuf;
    use tempfile::TempDir;

    fn create_test_resolved(temp_dir: &TempDir) -> Resolved {
        let queue_path = temp_dir.path().join("queue.json");
        let done_path = temp_dir.path().join("done.json");

        // Create empty queue file
        let queue = QueueFile::default();
        let queue_json = serde_json::to_string_pretty(&queue).unwrap();
        std::fs::write(&queue_path, queue_json).unwrap();

        Resolved {
            config: Config::default(),
            repo_root: temp_dir.path().to_path_buf(),
            queue_path,
            done_path,
            id_prefix: "RQ".to_string(),
            id_width: 4,
            global_config_path: None,
            project_config_path: None,
        }
    }

    #[test]
    fn generate_task_id_first_id_format() {
        let temp_dir = TempDir::new().unwrap();
        let resolved = create_test_resolved(&temp_dir);

        let task_id = generate_task_id(&resolved).unwrap();

        // First ID should be RQ-0001 (prefix + dash + zero-padded number)
        assert_eq!(task_id, "RQ-0001");
    }

    #[test]
    fn generate_task_id_considers_done_queue() {
        let temp_dir = TempDir::new().unwrap();
        let resolved = create_test_resolved(&temp_dir);

        // Add a task to done queue with high ID
        let mut done_queue = QueueFile::default();
        let now = crate::timeutil::now_utc_rfc3339_or_fallback();
        done_queue.tasks.push(Task {
            id: "RQ-0010".to_string(),
            status: TaskStatus::Done,
            title: "Old task".to_string(),
            description: None,
            priority: TaskPriority::Medium,
            tags: vec![],
            scope: vec![],
            evidence: vec![],
            plan: vec![],
            notes: vec![],
            request: None,
            agent: None,
            created_at: Some(now.clone()),
            updated_at: Some(now.clone()),
            completed_at: Some(now),
            started_at: None,
            scheduled_start: None,
            depends_on: vec![],
            blocks: vec![],
            relates_to: vec![],
            duplicates: None,
            custom_fields: HashMap::new(),
            parent_id: None,
        });
        let done_json = serde_json::to_string_pretty(&done_queue).unwrap();
        std::fs::write(&resolved.done_path, done_json).unwrap();

        let task_id = generate_task_id(&resolved).unwrap();

        // Next ID should be RQ-0011 (one higher than done queue)
        assert_eq!(task_id, "RQ-0011");
    }

    #[test]
    fn task_exists_for_comment_detects_duplicates() {
        let file_path = PathBuf::from("/test/file.rs");
        let comment = DetectedComment {
            file_path: file_path.clone(),
            line_number: 42,
            comment_type: CommentType::Todo,
            content: "fix this".to_string(),
            context: "file.rs:42 - fix this".to_string(),
        };

        let mut queue = QueueFile::default();
        queue.tasks.push(Task {
            id: "RQ-0001".to_string(),
            status: TaskStatus::Todo,
            title: "TODO: fix this in file.rs".to_string(),
            description: None,
            priority: TaskPriority::Medium,
            tags: vec![],
            scope: vec![],
            evidence: vec![],
            plan: vec![],
            notes: vec![format!("Detected in: {}:42", file_path.display())],
            request: None,
            agent: None,
            created_at: None,
            updated_at: None,
            completed_at: None,
            started_at: None,
            scheduled_start: None,
            depends_on: vec![],
            blocks: vec![],
            relates_to: vec![],
            duplicates: None,
            custom_fields: HashMap::new(),
            parent_id: None,
        });

        assert!(task_exists_for_comment(&queue, &comment));
    }

    #[test]
    fn task_exists_for_comment_no_false_positives() {
        let file_path = PathBuf::from("/test/file.rs");
        let comment = DetectedComment {
            file_path: file_path.clone(),
            line_number: 42,
            comment_type: CommentType::Todo,
            content: "fix this".to_string(),
            context: "file.rs:42 - fix this".to_string(),
        };

        let queue = QueueFile::default();

        assert!(!task_exists_for_comment(&queue, &comment));
    }

    // =================================================================
    // Fingerprint generation tests
    // =================================================================

    #[test]
    fn generate_comment_fingerprint_is_stable() {
        let content = "  Fix the error handling HERE  ";
        let fp1 = generate_comment_fingerprint(content);
        let fp2 = generate_comment_fingerprint(content);
        assert_eq!(fp1, fp2, "Fingerprint should be stable for same content");
        assert_eq!(fp1.len(), 16, "Fingerprint should be 16 characters");
    }

    #[test]
    fn generate_comment_fingerprint_normalizes_case() {
        let fp1 = generate_comment_fingerprint("TODO: Fix this");
        let fp2 = generate_comment_fingerprint("todo: fix this");
        assert_eq!(fp1, fp2, "Fingerprint should be case-insensitive");
    }

    #[test]
    fn generate_comment_fingerprint_normalizes_whitespace() {
        let fp1 = generate_comment_fingerprint("TODO: Fix this");
        let fp2 = generate_comment_fingerprint("  TODO: Fix this  ");
        assert_eq!(fp1, fp2, "Fingerprint should trim whitespace");
    }

    #[test]
    fn generate_comment_fingerprint_different_content() {
        let fp1 = generate_comment_fingerprint("TODO: Fix bug A");
        let fp2 = generate_comment_fingerprint("TODO: Fix bug B");
        assert_ne!(
            fp1, fp2,
            "Different content should have different fingerprints"
        );
    }

    // =================================================================
    // Fingerprint-based deduplication tests
    // =================================================================

    #[test]
    fn task_exists_for_comment_uses_fingerprint() {
        let file_path = PathBuf::from("/test/file.rs");
        let content = "fix the error handling";
        let fingerprint = generate_comment_fingerprint(content);

        let comment = DetectedComment {
            file_path: file_path.clone(),
            line_number: 42,
            comment_type: CommentType::Todo,
            content: content.to_string(),
            context: "file.rs:42 - fix this".to_string(),
        };

        let mut queue = QueueFile::default();
        let task = Task {
            id: "RQ-0001".to_string(),
            status: TaskStatus::Todo,
            title: "TODO: fix this in file.rs".to_string(),
            description: None,
            priority: TaskPriority::Medium,
            tags: vec!["watch".to_string(), "todo".to_string()],
            scope: vec![],
            evidence: vec![],
            plan: vec![],
            notes: vec![],
            request: None,
            agent: None,
            created_at: None,
            updated_at: None,
            completed_at: None,
            started_at: None,
            scheduled_start: None,
            depends_on: vec![],
            blocks: vec![],
            relates_to: vec![],
            duplicates: None,
            custom_fields: {
                let mut map = HashMap::new();
                map.insert(
                    "watch.file".to_string(),
                    file_path.to_string_lossy().to_string(),
                );
                map.insert("watch.line".to_string(), "100".to_string()); // Different line!
                map.insert("watch.fingerprint".to_string(), fingerprint);
                map.insert("watch.comment_type".to_string(), "todo".to_string());
                map
            },
            parent_id: None,
        };
        queue.tasks.push(task);

        // Should detect as duplicate even though line number differs
        // because fingerprint matches
        assert!(task_exists_for_comment(&queue, &comment));
    }

    #[test]
    fn task_exists_for_comment_respects_watch_tag() {
        let file_path = PathBuf::from("/test/file.rs");
        let content = "fix the error handling";
        let fingerprint = generate_comment_fingerprint(content);

        let comment = DetectedComment {
            file_path: file_path.clone(),
            line_number: 42,
            comment_type: CommentType::Todo,
            content: content.to_string(),
            context: "file.rs:42 - fix this".to_string(),
        };

        let mut queue = QueueFile::default();
        // Task without "watch" tag - should not use fingerprint matching
        let task = Task {
            id: "RQ-0001".to_string(),
            status: TaskStatus::Todo,
            title: "TODO: fix this in file.rs".to_string(),
            description: None,
            priority: TaskPriority::Medium,
            tags: vec!["todo".to_string()], // No "watch" tag
            scope: vec![],
            evidence: vec![],
            plan: vec![],
            notes: vec![format!("Detected in: {}:100", file_path.display())], // Different line
            request: None,
            agent: None,
            created_at: None,
            updated_at: None,
            completed_at: None,
            started_at: None,
            scheduled_start: None,
            depends_on: vec![],
            blocks: vec![],
            relates_to: vec![],
            duplicates: None,
            custom_fields: {
                let mut map = HashMap::new();
                map.insert("watch.fingerprint".to_string(), fingerprint);
                map
            },
            parent_id: None,
        };
        queue.tasks.push(task);

        // Should NOT detect as duplicate because it's not a watch task
        // (user-created task that happens to have fingerprint field)
        assert!(!task_exists_for_comment(&queue, &comment));
    }

    // =================================================================
    // Close-removed reconciliation tests
    // =================================================================

    #[test]
    fn reconcile_watch_tasks_closes_removed_comments() {
        let temp_dir = TempDir::new().unwrap();
        let resolved = create_test_resolved(&temp_dir);

        // Create a queue with a watch task
        let mut queue = QueueFile::default();
        let task = Task {
            id: "RQ-0001".to_string(),
            status: TaskStatus::Todo,
            title: "TODO: fix this".to_string(),
            description: None,
            priority: TaskPriority::Medium,
            tags: vec!["watch".to_string(), "todo".to_string()],
            scope: vec![],
            evidence: vec![],
            plan: vec![],
            notes: vec![],
            request: None,
            agent: None,
            created_at: Some("2026-01-01T00:00:00Z".to_string()),
            updated_at: Some("2026-01-01T00:00:00Z".to_string()),
            completed_at: None,
            started_at: None,
            scheduled_start: None,
            depends_on: vec![],
            blocks: vec![],
            relates_to: vec![],
            duplicates: None,
            custom_fields: {
                let mut map = HashMap::new();
                map.insert("watch.file".to_string(), "/test/file.rs".to_string());
                map.insert("watch.line".to_string(), "42".to_string());
                map.insert(
                    "watch.fingerprint".to_string(),
                    generate_comment_fingerprint("fix this"),
                );
                map.insert("watch.comment_type".to_string(), "todo".to_string());
                map
            },
            parent_id: None,
        };
        queue.tasks.push(task);
        save_queue(&resolved.queue_path, &queue).unwrap();

        // No comments detected (simulating comment removal)
        let detected_comments: Vec<DetectedComment> = vec![];

        let opts = WatchOptions {
            patterns: vec!["*.rs".to_string()],
            debounce_ms: 100,
            auto_queue: false,
            notify: false,
            ignore_patterns: vec![],
            comment_types: vec![CommentType::Todo],
            paths: vec![PathBuf::from(".")],
            force: false,
            close_removed: true, // Enable auto-close
        };

        let closed = reconcile_watch_tasks(&resolved, &detected_comments, &opts).unwrap();

        assert_eq!(closed.len(), 1);
        assert_eq!(closed[0], "RQ-0001");

        // Verify task was marked done
        let updated_queue = load_queue(&resolved.queue_path).unwrap();
        assert_eq!(updated_queue.tasks[0].status, TaskStatus::Done);
        assert!(updated_queue.tasks[0].completed_at.is_some());
        assert!(
            updated_queue.tasks[0]
                .notes
                .iter()
                .any(|n| n.contains("Automatically marked done"))
        );
    }

    #[test]
    fn reconcile_watch_tasks_preserves_existing_comments() {
        let temp_dir = TempDir::new().unwrap();
        let resolved = create_test_resolved(&temp_dir);

        // Create a queue with a watch task
        let mut queue = QueueFile::default();
        let fingerprint = generate_comment_fingerprint("fix this");
        let task = Task {
            id: "RQ-0001".to_string(),
            status: TaskStatus::Todo,
            title: "TODO: fix this".to_string(),
            description: None,
            priority: TaskPriority::Medium,
            tags: vec!["watch".to_string()],
            scope: vec![],
            evidence: vec![],
            plan: vec![],
            notes: vec![],
            request: None,
            agent: None,
            created_at: Some("2026-01-01T00:00:00Z".to_string()),
            updated_at: Some("2026-01-01T00:00:00Z".to_string()),
            completed_at: None,
            started_at: None,
            scheduled_start: None,
            depends_on: vec![],
            blocks: vec![],
            relates_to: vec![],
            duplicates: None,
            custom_fields: {
                let mut map = HashMap::new();
                map.insert("watch.file".to_string(), "/test/file.rs".to_string());
                map.insert("watch.line".to_string(), "42".to_string());
                map.insert("watch.fingerprint".to_string(), fingerprint.clone());
                map.insert("watch.comment_type".to_string(), "todo".to_string());
                map
            },
            parent_id: None,
        };
        queue.tasks.push(task);
        save_queue(&resolved.queue_path, &queue).unwrap();

        // Comment still exists (but moved to different line)
        let detected_comments = vec![DetectedComment {
            file_path: PathBuf::from("/test/file.rs"),
            line_number: 100, // Different line - moved!
            comment_type: CommentType::Todo,
            content: "fix this".to_string(),
            context: "test".to_string(),
        }];

        let opts = WatchOptions {
            patterns: vec!["*.rs".to_string()],
            debounce_ms: 100,
            auto_queue: false,
            notify: false,
            ignore_patterns: vec![],
            comment_types: vec![CommentType::Todo],
            paths: vec![PathBuf::from(".")],
            force: false,
            close_removed: true,
        };

        let closed = reconcile_watch_tasks(&resolved, &detected_comments, &opts).unwrap();

        // Task should NOT be closed because fingerprint still matches (comment moved, not removed)
        assert!(closed.is_empty());

        let updated_queue = load_queue(&resolved.queue_path).unwrap();
        assert_eq!(updated_queue.tasks[0].status, TaskStatus::Todo);
    }

    #[test]
    fn reconcile_watch_tasks_skips_non_watch_tasks() {
        let temp_dir = TempDir::new().unwrap();
        let resolved = create_test_resolved(&temp_dir);

        // Create a queue with a user-created task (no watch tag)
        let mut queue = QueueFile::default();
        let task = Task {
            id: "RQ-0001".to_string(),
            status: TaskStatus::Todo,
            title: "User task".to_string(),
            description: None,
            priority: TaskPriority::Medium,
            tags: vec!["user".to_string()], // No "watch" tag
            scope: vec![],
            evidence: vec![],
            plan: vec![],
            notes: vec![],
            request: None,
            agent: None,
            created_at: Some("2026-01-01T00:00:00Z".to_string()),
            updated_at: Some("2026-01-01T00:00:00Z".to_string()),
            completed_at: None,
            started_at: None,
            scheduled_start: None,
            depends_on: vec![],
            blocks: vec![],
            relates_to: vec![],
            duplicates: None,
            custom_fields: HashMap::new(),
            parent_id: None,
        };
        queue.tasks.push(task);
        save_queue(&resolved.queue_path, &queue).unwrap();

        let detected_comments: Vec<DetectedComment> = vec![];

        let opts = WatchOptions {
            patterns: vec!["*.rs".to_string()],
            debounce_ms: 100,
            auto_queue: false,
            notify: false,
            ignore_patterns: vec![],
            comment_types: vec![CommentType::Todo],
            paths: vec![PathBuf::from(".")],
            force: false,
            close_removed: true,
        };

        let closed = reconcile_watch_tasks(&resolved, &detected_comments, &opts).unwrap();

        // Non-watch task should NOT be closed
        assert!(closed.is_empty());

        let updated_queue = load_queue(&resolved.queue_path).unwrap();
        assert_eq!(updated_queue.tasks[0].status, TaskStatus::Todo);
    }

    #[test]
    fn reconcile_watch_tasks_skips_already_closed_tasks() {
        let temp_dir = TempDir::new().unwrap();
        let resolved = create_test_resolved(&temp_dir);

        // Create a queue with a done watch task
        let mut queue = QueueFile::default();
        let task = Task {
            id: "RQ-0001".to_string(),
            status: TaskStatus::Done, // Already done
            title: "TODO: fix this".to_string(),
            description: None,
            priority: TaskPriority::Medium,
            tags: vec!["watch".to_string()],
            scope: vec![],
            evidence: vec![],
            plan: vec![],
            notes: vec![],
            request: None,
            agent: None,
            created_at: Some("2026-01-01T00:00:00Z".to_string()),
            updated_at: Some("2026-01-01T00:00:00Z".to_string()),
            completed_at: Some("2026-01-02T00:00:00Z".to_string()),
            started_at: None,
            scheduled_start: None,
            depends_on: vec![],
            blocks: vec![],
            relates_to: vec![],
            duplicates: None,
            custom_fields: {
                let mut map = HashMap::new();
                map.insert(
                    "watch.fingerprint".to_string(),
                    generate_comment_fingerprint("fix this"),
                );
                map
            },
            parent_id: None,
        };
        queue.tasks.push(task);
        save_queue(&resolved.queue_path, &queue).unwrap();

        let detected_comments: Vec<DetectedComment> = vec![];

        let opts = WatchOptions {
            patterns: vec!["*.rs".to_string()],
            debounce_ms: 100,
            auto_queue: false,
            notify: false,
            ignore_patterns: vec![],
            comment_types: vec![CommentType::Todo],
            paths: vec![PathBuf::from(".")],
            force: false,
            close_removed: true,
        };

        let closed = reconcile_watch_tasks(&resolved, &detected_comments, &opts).unwrap();

        // Already done task should NOT be processed again
        assert!(closed.is_empty());
    }

    #[test]
    fn create_task_from_comment_populates_custom_fields() {
        let temp_dir = TempDir::new().unwrap();
        let resolved = create_test_resolved(&temp_dir);

        let comment = DetectedComment {
            file_path: PathBuf::from("/test/file.rs"),
            line_number: 42,
            comment_type: CommentType::Todo,
            content: "Fix the error handling".to_string(),
            context: "fn foo() {".to_string(),
        };

        let task = create_task_from_comment(&comment, &resolved).unwrap();

        // Verify custom_fields are populated
        assert_eq!(
            task.custom_fields.get("watch.file"),
            Some(&"/test/file.rs".to_string())
        );
        assert_eq!(
            task.custom_fields.get("watch.line"),
            Some(&"42".to_string())
        );
        assert_eq!(
            task.custom_fields.get("watch.comment_type"),
            Some(&"todo".to_string())
        );
        assert_eq!(
            task.custom_fields.get("watch.version"),
            Some(&"1".to_string())
        );

        // Verify fingerprint is generated
        let fingerprint = task.custom_fields.get("watch.fingerprint").unwrap();
        assert_eq!(fingerprint.len(), 16);

        // Verify fingerprint matches expected value
        let expected_fingerprint = generate_comment_fingerprint("Fix the error handling");
        assert_eq!(fingerprint, &expected_fingerprint);

        // Verify tags include "watch" and comment type
        assert!(task.tags.contains(&"watch".to_string()));
        assert!(task.tags.contains(&"todo".to_string()));
    }
}
