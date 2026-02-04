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
//! - Deduplication checks file path and line number in task title/notes.
//! - Queue is loaded and saved atomically within this module.

use crate::commands::watch::types::{DetectedComment, WatchOptions};
use crate::config::Resolved;
use crate::contracts::{QueueFile, Task, TaskPriority, TaskStatus};
use crate::notification::{NotificationConfig, notify_watch_new_task};
use crate::queue::{load_queue, load_queue_or_default, save_queue, suggest_new_task_insert_index};
use crate::timeutil;
use anyhow::Result;
use std::collections::HashMap;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::Path;

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

    Ok(())
}

/// Generate a stable fingerprint for a comment.
/// Uses: file_path (relative) + line_number + normalized_content_hash
fn generate_comment_fingerprint(file_path: &Path, line_number: usize, content: &str) -> String {
    // Normalize content: lowercase, trim whitespace, collapse multiple spaces
    let normalized = content
        .to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");

    // Use relative path for portability across machines
    let relative_path = file_path
        .strip_prefix(std::env::current_dir().unwrap_or_default())
        .unwrap_or(file_path)
        .to_string_lossy();

    let mut hasher = DefaultHasher::new();
    relative_path.hash(&mut hasher);
    line_number.hash(&mut hasher);
    normalized.hash(&mut hasher);

    format!("{:016x}", hasher.finish())
}

/// Check if a task already exists for a given comment.
pub fn task_exists_for_comment(queue: &QueueFile, comment: &DetectedComment) -> bool {
    let fingerprint =
        generate_comment_fingerprint(&comment.file_path, comment.line_number, &comment.content);

    queue.tasks.iter().any(|task| {
        // Primary: Check fingerprint in custom_fields
        if let Some(existing_fp) = task.custom_fields.get("watch.fingerprint")
            && existing_fp == &fingerprint
        {
            return true;
        }

        // Fallback: Legacy substring matching for backwards compatibility
        // with tasks created before this change
        let file_str = comment.file_path.to_string_lossy().to_string();
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

    // Generate fingerprint and populate custom_fields
    let fingerprint =
        generate_comment_fingerprint(&comment.file_path, comment.line_number, &comment.content);
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

/// Reconcile watch tasks by closing those whose comments no longer exist.
/// Only affects tasks with the "watch" tag to avoid mutating user-authored tasks.
pub fn reconcile_removed_comments(
    resolved: &Resolved,
    detected_comments: &[DetectedComment],
    opts: &WatchOptions,
) -> Result<Vec<String>> {
    if !opts.close_removed {
        return Ok(Vec::new());
    }

    let mut queue = load_queue(&resolved.queue_path)?;
    let now = timeutil::now_utc_rfc3339_or_fallback();
    let mut closed_tasks = Vec::new();

    // Build set of current fingerprints
    let current_fingerprints: std::collections::HashSet<String> = detected_comments
        .iter()
        .map(|c| generate_comment_fingerprint(&c.file_path, c.line_number, &c.content))
        .collect();

    for task in &mut queue.tasks {
        // Only process watch-created tasks that are still active
        if !task.tags.contains(&"watch".to_string()) {
            continue;
        }
        if !matches!(
            task.status,
            TaskStatus::Todo | TaskStatus::Doing | TaskStatus::Draft
        ) {
            continue;
        }

        // Check if this task has a fingerprint
        if let Some(fp) = task.custom_fields.get("watch.fingerprint")
            && !current_fingerprints.contains(fp)
        {
            // Comment no longer exists - mark as done
            task.status = TaskStatus::Done;
            task.completed_at = Some(now.clone());
            task.updated_at = Some(now.clone());
            task.notes.push(format!(
                "[{}] Auto-closed: originating comment was removed from source",
                now
            ));
            closed_tasks.push(task.id.clone());
            log::info!("Closed task {}: comment removed from source", task.id);
        }
    }

    if !closed_tasks.is_empty() {
        save_queue(&resolved.queue_path, &queue)?;
        log::info!(
            "Closed {} task(s) due to removed comments",
            closed_tasks.len()
        );
    }

    Ok(closed_tasks)
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

    // Fingerprint tests
    #[test]
    fn fingerprint_is_stable_for_same_comment() {
        let fp1 = generate_comment_fingerprint(
            Path::new("/project/src/main.rs"),
            42,
            "TODO: fix this bug",
        );
        let fp2 = generate_comment_fingerprint(
            Path::new("/project/src/main.rs"),
            42,
            "TODO: fix this bug",
        );
        assert_eq!(fp1, fp2);
    }

    #[test]
    fn fingerprint_changes_with_content() {
        let fp1 = generate_comment_fingerprint(Path::new("src/main.rs"), 42, "TODO: fix A");
        let fp2 = generate_comment_fingerprint(Path::new("src/main.rs"), 42, "TODO: fix B");
        assert_ne!(fp1, fp2);
    }

    #[test]
    fn fingerprint_changes_with_line() {
        let fp1 = generate_comment_fingerprint(Path::new("src/main.rs"), 42, "TODO: fix");
        let fp2 = generate_comment_fingerprint(Path::new("src/main.rs"), 43, "TODO: fix");
        assert_ne!(fp1, fp2);
    }

    #[test]
    fn fingerprint_ignores_whitespace_normalization() {
        let fp1 = generate_comment_fingerprint(Path::new("src/main.rs"), 42, "TODO:   fix   this");
        let fp2 = generate_comment_fingerprint(Path::new("src/main.rs"), 42, "TODO: fix this");
        assert_eq!(fp1, fp2);
    }

    #[test]
    fn task_exists_uses_fingerprint_in_custom_fields() {
        // Test that fingerprint matching takes priority
        let comment = DetectedComment {
            file_path: PathBuf::from("src/main.rs"),
            line_number: 42,
            comment_type: CommentType::Todo,
            content: "fix this".to_string(),
            context: "context".to_string(),
        };

        let mut queue = QueueFile::default();
        let mut task = create_test_task();
        task.custom_fields.insert(
            "watch.fingerprint".to_string(),
            generate_comment_fingerprint(&comment.file_path, comment.line_number, &comment.content),
        );
        queue.tasks.push(task);

        assert!(task_exists_for_comment(&queue, &comment));
    }

    #[test]
    fn task_exists_fallback_to_legacy_matching() {
        // Test that legacy substring matching still works for tasks without fingerprint
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
            scheduled_start: None,
            depends_on: vec![],
            blocks: vec![],
            relates_to: vec![],
            duplicates: None,
            custom_fields: HashMap::new(), // No fingerprint - should use fallback
            parent_id: None,
        });

        assert!(task_exists_for_comment(&queue, &comment));
    }

    // Reconciliation tests
    #[test]
    fn reconcile_closes_tasks_for_removed_comments() {
        let temp_dir = TempDir::new().unwrap();
        let resolved = create_test_resolved(&temp_dir);

        // Create a watch task with fingerprint
        let file_path = PathBuf::from("src/main.rs");
        let fingerprint = generate_comment_fingerprint(&file_path, 42, "TODO: fix this");

        let mut queue = QueueFile::default();
        queue.tasks.push(Task {
            id: "RQ-0001".to_string(),
            status: TaskStatus::Todo,
            title: "TODO: fix this in main.rs".to_string(),
            priority: TaskPriority::Medium,
            tags: vec!["watch".to_string()],
            scope: vec![],
            evidence: vec![],
            plan: vec![],
            notes: vec![],
            request: None,
            agent: None,
            created_at: None,
            updated_at: None,
            completed_at: None,
            scheduled_start: None,
            depends_on: vec![],
            blocks: vec![],
            relates_to: vec![],
            duplicates: None,
            custom_fields: {
                let mut fields = HashMap::new();
                fields.insert("watch.fingerprint".to_string(), fingerprint);
                fields.insert(
                    "watch.file".to_string(),
                    file_path.to_string_lossy().to_string(),
                );
                fields.insert("watch.line".to_string(), "42".to_string());
                fields
            },
            parent_id: None,
        });
        save_queue(&resolved.queue_path, &queue).unwrap();

        // Call reconcile with empty detected comments (simulating removed comment)
        let opts = WatchOptions {
            patterns: vec![],
            debounce_ms: 500,
            auto_queue: false,
            notify: false,
            ignore_patterns: vec![],
            comment_types: vec![CommentType::Todo],
            paths: vec![],
            force: false,
            close_removed: true,
        };
        let closed = reconcile_removed_comments(&resolved, &[], &opts).unwrap();

        // Assert task was closed
        assert_eq!(closed.len(), 1);
        assert_eq!(closed[0], "RQ-0001");

        // Verify task status in queue
        let updated_queue = load_queue(&resolved.queue_path).unwrap();
        assert_eq!(updated_queue.tasks[0].status, TaskStatus::Done);
        assert!(updated_queue.tasks[0].completed_at.is_some());
        assert!(
            updated_queue.tasks[0]
                .notes
                .iter()
                .any(|n| n.contains("Auto-closed"))
        );
    }

    #[test]
    fn reconcile_only_affects_watch_tagged_tasks() {
        let temp_dir = TempDir::new().unwrap();
        let resolved = create_test_resolved(&temp_dir);

        // Create a task without "watch" tag
        let file_path = PathBuf::from("src/main.rs");
        let fingerprint = generate_comment_fingerprint(&file_path, 42, "TODO: fix this");

        let mut queue = QueueFile::default();
        queue.tasks.push(Task {
            id: "RQ-0001".to_string(),
            status: TaskStatus::Todo,
            title: "TODO: fix this in main.rs".to_string(),
            priority: TaskPriority::Medium,
            tags: vec!["manual".to_string()], // No "watch" tag
            scope: vec![],
            evidence: vec![],
            plan: vec![],
            notes: vec![],
            request: None,
            agent: None,
            created_at: None,
            updated_at: None,
            completed_at: None,
            scheduled_start: None,
            depends_on: vec![],
            blocks: vec![],
            relates_to: vec![],
            duplicates: None,
            custom_fields: {
                let mut fields = HashMap::new();
                fields.insert("watch.fingerprint".to_string(), fingerprint);
                fields
            },
            parent_id: None,
        });
        save_queue(&resolved.queue_path, &queue).unwrap();

        // Call reconcile with empty detected comments
        let opts = WatchOptions {
            patterns: vec![],
            debounce_ms: 500,
            auto_queue: false,
            notify: false,
            ignore_patterns: vec![],
            comment_types: vec![CommentType::Todo],
            paths: vec![],
            force: false,
            close_removed: true,
        };
        let closed = reconcile_removed_comments(&resolved, &[], &opts).unwrap();

        // Assert no tasks were closed
        assert!(closed.is_empty());

        // Verify task status unchanged
        let updated_queue = load_queue(&resolved.queue_path).unwrap();
        assert_eq!(updated_queue.tasks[0].status, TaskStatus::Todo);
    }

    #[test]
    fn reconcile_only_affects_active_tasks() {
        let temp_dir = TempDir::new().unwrap();
        let resolved = create_test_resolved(&temp_dir);

        // Create a watch task that's already done
        let file_path = PathBuf::from("src/main.rs");
        let fingerprint = generate_comment_fingerprint(&file_path, 42, "TODO: fix this");

        let mut queue = QueueFile::default();
        queue.tasks.push(Task {
            id: "RQ-0001".to_string(),
            status: TaskStatus::Done, // Already done
            title: "TODO: fix this in main.rs".to_string(),
            priority: TaskPriority::Medium,
            tags: vec!["watch".to_string()],
            scope: vec![],
            evidence: vec![],
            plan: vec![],
            notes: vec![],
            request: None,
            agent: None,
            created_at: None,
            updated_at: None,
            completed_at: Some("2024-01-01T00:00:00Z".to_string()),
            scheduled_start: None,
            depends_on: vec![],
            blocks: vec![],
            relates_to: vec![],
            duplicates: None,
            custom_fields: {
                let mut fields = HashMap::new();
                fields.insert("watch.fingerprint".to_string(), fingerprint);
                fields
            },
            parent_id: None,
        });
        save_queue(&resolved.queue_path, &queue).unwrap();

        // Call reconcile with empty detected comments
        let opts = WatchOptions {
            patterns: vec![],
            debounce_ms: 500,
            auto_queue: false,
            notify: false,
            ignore_patterns: vec![],
            comment_types: vec![CommentType::Todo],
            paths: vec![],
            force: false,
            close_removed: true,
        };
        let closed = reconcile_removed_comments(&resolved, &[], &opts).unwrap();

        // Assert no tasks were closed (already done)
        assert!(closed.is_empty());
    }

    #[test]
    fn reconcile_skips_when_close_removed_disabled() {
        let temp_dir = TempDir::new().unwrap();
        let resolved = create_test_resolved(&temp_dir);

        // Create a watch task with fingerprint
        let file_path = PathBuf::from("src/main.rs");
        let fingerprint = generate_comment_fingerprint(&file_path, 42, "TODO: fix this");

        let mut queue = QueueFile::default();
        queue.tasks.push(Task {
            id: "RQ-0001".to_string(),
            status: TaskStatus::Todo,
            title: "TODO: fix this in main.rs".to_string(),
            priority: TaskPriority::Medium,
            tags: vec!["watch".to_string()],
            scope: vec![],
            evidence: vec![],
            plan: vec![],
            notes: vec![],
            request: None,
            agent: None,
            created_at: None,
            updated_at: None,
            completed_at: None,
            scheduled_start: None,
            depends_on: vec![],
            blocks: vec![],
            relates_to: vec![],
            duplicates: None,
            custom_fields: {
                let mut fields = HashMap::new();
                fields.insert("watch.fingerprint".to_string(), fingerprint);
                fields
            },
            parent_id: None,
        });
        save_queue(&resolved.queue_path, &queue).unwrap();

        // Call reconcile with close_removed disabled
        let opts = WatchOptions {
            patterns: vec![],
            debounce_ms: 500,
            auto_queue: false,
            notify: false,
            ignore_patterns: vec![],
            comment_types: vec![CommentType::Todo],
            paths: vec![],
            force: false,
            close_removed: false, // Disabled
        };
        let closed = reconcile_removed_comments(&resolved, &[], &opts).unwrap();

        // Assert no tasks were closed
        assert!(closed.is_empty());

        // Verify task status unchanged
        let updated_queue = load_queue(&resolved.queue_path).unwrap();
        assert_eq!(updated_queue.tasks[0].status, TaskStatus::Todo);
    }

    #[test]
    fn reconcile_keeps_tasks_with_existing_comments() {
        let temp_dir = TempDir::new().unwrap();
        let resolved = create_test_resolved(&temp_dir);

        // Create a watch task with fingerprint
        let file_path = PathBuf::from("src/main.rs");
        let content = "TODO: fix this";
        let fingerprint = generate_comment_fingerprint(&file_path, 42, content);

        let mut queue = QueueFile::default();
        queue.tasks.push(Task {
            id: "RQ-0001".to_string(),
            status: TaskStatus::Todo,
            title: "TODO: fix this in main.rs".to_string(),
            priority: TaskPriority::Medium,
            tags: vec!["watch".to_string()],
            scope: vec![],
            evidence: vec![],
            plan: vec![],
            notes: vec![],
            request: None,
            agent: None,
            created_at: None,
            updated_at: None,
            completed_at: None,
            scheduled_start: None,
            depends_on: vec![],
            blocks: vec![],
            relates_to: vec![],
            duplicates: None,
            custom_fields: {
                let mut fields = HashMap::new();
                fields.insert("watch.fingerprint".to_string(), fingerprint);
                fields
            },
            parent_id: None,
        });
        save_queue(&resolved.queue_path, &queue).unwrap();

        // Create a detected comment matching the task
        let detected = DetectedComment {
            file_path: file_path.clone(),
            line_number: 42,
            comment_type: CommentType::Todo,
            content: content.to_string(),
            context: "context".to_string(),
        };

        // Call reconcile with the existing comment
        let opts = WatchOptions {
            patterns: vec![],
            debounce_ms: 500,
            auto_queue: false,
            notify: false,
            ignore_patterns: vec![],
            comment_types: vec![CommentType::Todo],
            paths: vec![],
            force: false,
            close_removed: true,
        };
        let closed = reconcile_removed_comments(&resolved, &[detected], &opts).unwrap();

        // Assert no tasks were closed (comment still exists)
        assert!(closed.is_empty());

        // Verify task status unchanged
        let updated_queue = load_queue(&resolved.queue_path).unwrap();
        assert_eq!(updated_queue.tasks[0].status, TaskStatus::Todo);
    }

    // Helper function to create a test task
    fn create_test_task() -> Task {
        Task {
            id: "RQ-0001".to_string(),
            status: TaskStatus::Todo,
            title: "Test task".to_string(),
            priority: TaskPriority::Medium,
            tags: vec![],
            scope: vec![],
            evidence: vec![],
            plan: vec![],
            notes: vec![],
            request: None,
            agent: None,
            created_at: None,
            updated_at: None,
            completed_at: None,
            scheduled_start: None,
            depends_on: vec![],
            blocks: vec![],
            relates_to: vec![],
            duplicates: None,
            custom_fields: HashMap::new(),
            parent_id: None,
        }
    }
}
