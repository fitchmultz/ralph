//! Watch task materialization.
//!
//! Purpose:
//! - Watch task materialization.
//!
//! Responsibilities:
//! - Convert detected comments into queue tasks with watch metadata.
//! - Generate unique task IDs for watch-created tasks.
//!
//! Not handled here:
//! - Queue reconciliation and duplicate detection.
//! - File watching orchestration.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - New watch tasks always use watch identity version 2 metadata.

use std::collections::HashMap;

use anyhow::Result;

use crate::commands::watch::identity::{
    WATCH_FIELD_COMMENT_TYPE, WATCH_FIELD_CONTENT_HASH, WATCH_FIELD_FILE, WATCH_FIELD_FINGERPRINT,
    WATCH_FIELD_IDENTITY_KEY, WATCH_FIELD_LINE, WATCH_FIELD_LOCATION_KEY, WATCH_FIELD_VERSION,
    WATCH_VERSION_V2, WatchCommentIdentity,
};
use crate::commands::watch::types::DetectedComment;
use crate::config::Resolved;
use crate::contracts::{Task, TaskPriority, TaskStatus};
use crate::queue::load_queue_or_default;
use crate::timeutil;

pub(super) fn generate_task_id(resolved: &Resolved) -> Result<String> {
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

pub fn create_task_from_comment(comment: &DetectedComment, resolved: &Resolved) -> Result<Task> {
    let type_str = format!("{:?}", comment.comment_type).to_uppercase();
    let file_name = comment
        .file_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("unknown");
    let identity = WatchCommentIdentity::from_detected_comment(comment);
    let now = timeutil::now_utc_rfc3339_or_fallback();

    let mut custom_fields = HashMap::new();
    custom_fields.insert(
        WATCH_FIELD_VERSION.to_string(),
        WATCH_VERSION_V2.to_string(),
    );
    custom_fields.insert(WATCH_FIELD_FILE.to_string(), identity.file.clone());
    custom_fields.insert(WATCH_FIELD_LINE.to_string(), identity.line.to_string());
    custom_fields.insert(
        WATCH_FIELD_COMMENT_TYPE.to_string(),
        identity.comment_type.clone(),
    );
    custom_fields.insert(
        WATCH_FIELD_CONTENT_HASH.to_string(),
        identity.content_hash.clone(),
    );
    custom_fields.insert(
        WATCH_FIELD_LOCATION_KEY.to_string(),
        identity.location_key.clone(),
    );
    custom_fields.insert(
        WATCH_FIELD_IDENTITY_KEY.to_string(),
        identity.identity_key.clone(),
    );
    custom_fields.insert(
        WATCH_FIELD_FINGERPRINT.to_string(),
        identity.content_hash.clone(),
    );

    Ok(Task {
        id: generate_task_id(resolved)?,
        status: TaskStatus::Todo,
        title: format!(
            "{}: {} in {}",
            type_str,
            comment.content.chars().take(50).collect::<String>(),
            file_name
        ),
        description: None,
        priority: TaskPriority::Medium,
        tags: vec![
            "watch".to_string(),
            format!("{:?}", comment.comment_type).to_lowercase(),
        ],
        scope: vec![identity.file.clone()],
        evidence: Vec::new(),
        plan: Vec::new(),
        notes: vec![
            format!(
                "Detected in: {}:{}",
                comment.file_path.display(),
                comment.line_number
            ),
            format!("Full content: {}", comment.content),
            format!("Context: {}", comment.context),
        ],
        request: Some(format!("Address {} comment", type_str)),
        agent: None,
        created_at: Some(now.clone()),
        updated_at: Some(now),
        completed_at: None,
        started_at: None,
        estimated_minutes: None,
        actual_minutes: None,
        scheduled_start: None,
        depends_on: Vec::new(),
        blocks: Vec::new(),
        relates_to: Vec::new(),
        duplicates: None,
        custom_fields,
        parent_id: None,
    })
}
