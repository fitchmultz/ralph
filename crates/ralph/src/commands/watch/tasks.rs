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

/// Check if a task already exists for a given comment.
pub fn task_exists_for_comment(queue: &QueueFile, comment: &DetectedComment) -> bool {
    let file_str = comment.file_path.to_string_lossy().to_string();

    queue.tasks.iter().any(|task| {
        // Check if task title or notes reference this file and line
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
        custom_fields: HashMap::new(),
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
}
