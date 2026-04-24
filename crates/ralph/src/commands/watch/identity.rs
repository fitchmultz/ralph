//! Watch identity helpers for the watch command.
//!
//! Purpose:
//! - Watch identity helpers for the watch command.
//!
//! Responsibilities:
//! - Define the versioned metadata contract for watch-created tasks.
//! - Build deterministic identities from detected comments.
//! - Parse stored watch metadata for V2 and structured legacy tasks.
//!
//! Not handled here:
//! - Queue loading or saving.
//! - File watching or comment detection.
//! - Task creation and reconciliation orchestration.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - V2 identity is based on normalized path, line, comment type, and content hash.
//! - Legacy tasks are only matched through structured metadata.
//! - Unstructured legacy watch tasks are never heuristically matched.

use crate::commands::watch::types::{CommentType, DetectedComment};
use crate::contracts::Task;
use sha2::{Digest, Sha256};
use std::path::Path;

pub const WATCH_VERSION_V2: &str = "2";
pub const WATCH_FIELD_VERSION: &str = "watch.version";
pub const WATCH_FIELD_FILE: &str = "watch.file";
pub const WATCH_FIELD_LINE: &str = "watch.line";
pub const WATCH_FIELD_COMMENT_TYPE: &str = "watch.comment_type";
pub const WATCH_FIELD_CONTENT_HASH: &str = "watch.content_hash";
pub const WATCH_FIELD_LOCATION_KEY: &str = "watch.location_key";
pub const WATCH_FIELD_IDENTITY_KEY: &str = "watch.identity_key";
pub const WATCH_FIELD_FINGERPRINT: &str = "watch.fingerprint";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WatchCommentIdentity {
    pub file: String,
    pub line: usize,
    pub comment_type: String,
    pub content_hash: String,
    pub location_key: String,
    pub identity_key: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LegacyWatchIdentity {
    pub file: String,
    pub line: usize,
    pub comment_type: Option<String>,
    pub content_hash: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParsedWatchIdentity {
    V2(WatchCommentIdentity),
    LegacyStructured(LegacyWatchIdentity),
    LegacyUnstructured,
}

impl WatchCommentIdentity {
    pub fn from_detected_comment(comment: &DetectedComment) -> Self {
        let file = path_key(&comment.file_path);
        let line = comment.line_number;
        let comment_type = comment_type_key(comment.comment_type);
        let content_hash = generate_comment_fingerprint(&comment.content);
        let location_key = generate_comment_location_key(&file, line);
        let identity_key = generate_comment_identity_key(&file, line, &comment_type, &content_hash);

        Self {
            file,
            line,
            comment_type,
            content_hash,
            location_key,
            identity_key,
        }
    }
}

impl LegacyWatchIdentity {
    pub fn matches_comment(&self, current: &WatchCommentIdentity) -> bool {
        if self.file != current.file || self.line != current.line {
            return false;
        }

        if let Some(comment_type) = self.comment_type.as_deref()
            && comment_type != current.comment_type
        {
            return false;
        }

        match self.content_hash.as_deref() {
            Some(content_hash) => content_hash == current.content_hash,
            None => true,
        }
    }
}

pub fn parse_task_watch_identity(task: &Task) -> Option<ParsedWatchIdentity> {
    if !task.tags.iter().any(|tag| tag == "watch") {
        return None;
    }

    let version = task
        .custom_fields
        .get(WATCH_FIELD_VERSION)
        .map(String::as_str);
    let file = task
        .custom_fields
        .get(WATCH_FIELD_FILE)
        .map(|value| normalize_path_str(value));
    let line = task
        .custom_fields
        .get(WATCH_FIELD_LINE)
        .and_then(|value| value.parse::<usize>().ok());
    let comment_type = task
        .custom_fields
        .get(WATCH_FIELD_COMMENT_TYPE)
        .map(|value| value.trim().to_lowercase());
    let content_hash = task
        .custom_fields
        .get(WATCH_FIELD_CONTENT_HASH)
        .cloned()
        .or_else(|| task.custom_fields.get(WATCH_FIELD_FINGERPRINT).cloned());

    match version {
        Some(WATCH_VERSION_V2) => {
            let (file, line, comment_type, content_hash) =
                match (file, line, comment_type, content_hash) {
                    (Some(file), Some(line), Some(comment_type), Some(content_hash)) => {
                        (file, line, comment_type, content_hash)
                    }
                    _ => return Some(ParsedWatchIdentity::LegacyUnstructured),
                };

            let location_key = task
                .custom_fields
                .get(WATCH_FIELD_LOCATION_KEY)
                .cloned()
                .unwrap_or_else(|| generate_comment_location_key(&file, line));
            let identity_key = task
                .custom_fields
                .get(WATCH_FIELD_IDENTITY_KEY)
                .cloned()
                .unwrap_or_else(|| {
                    generate_comment_identity_key(&file, line, &comment_type, &content_hash)
                });

            Some(ParsedWatchIdentity::V2(WatchCommentIdentity {
                file,
                line,
                comment_type,
                content_hash,
                location_key,
                identity_key,
            }))
        }
        _ => match (file, line) {
            (Some(file), Some(line)) => {
                Some(ParsedWatchIdentity::LegacyStructured(LegacyWatchIdentity {
                    file,
                    line,
                    comment_type,
                    content_hash,
                }))
            }
            _ => Some(ParsedWatchIdentity::LegacyUnstructured),
        },
    }
}

pub fn upgrade_task_to_v2(task: &mut Task, identity: &WatchCommentIdentity) {
    task.custom_fields.insert(
        WATCH_FIELD_VERSION.to_string(),
        WATCH_VERSION_V2.to_string(),
    );
    task.custom_fields
        .insert(WATCH_FIELD_FILE.to_string(), identity.file.clone());
    task.custom_fields
        .insert(WATCH_FIELD_LINE.to_string(), identity.line.to_string());
    task.custom_fields.insert(
        WATCH_FIELD_COMMENT_TYPE.to_string(),
        identity.comment_type.clone(),
    );
    task.custom_fields.insert(
        WATCH_FIELD_CONTENT_HASH.to_string(),
        identity.content_hash.clone(),
    );
    task.custom_fields.insert(
        WATCH_FIELD_LOCATION_KEY.to_string(),
        identity.location_key.clone(),
    );
    task.custom_fields.insert(
        WATCH_FIELD_IDENTITY_KEY.to_string(),
        identity.identity_key.clone(),
    );
    task.custom_fields.insert(
        WATCH_FIELD_FINGERPRINT.to_string(),
        identity.content_hash.clone(),
    );
}

pub fn path_key(path: &Path) -> String {
    normalize_path_str(&path.to_string_lossy())
}

pub fn comment_type_key(comment_type: CommentType) -> String {
    format!("{comment_type:?}").to_lowercase()
}

pub fn generate_comment_content_hash(content: &str) -> String {
    let normalized = content.trim().to_lowercase();
    hash16(&normalized)
}

pub fn generate_comment_location_key(file: &str, line: usize) -> String {
    hash16(&format!("{file}:{line}"))
}

pub fn generate_comment_identity_key(
    file: &str,
    line: usize,
    comment_type: &str,
    content_hash: &str,
) -> String {
    hash16(&format!("{file}:{line}:{comment_type}:{content_hash}"))
}

pub fn generate_comment_fingerprint(content: &str) -> String {
    generate_comment_content_hash(content)
}

fn normalize_path_str(path: &str) -> String {
    path.replace('\\', "/")
}

fn hash16(value: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(value.as_bytes());
    hex::encode(hasher.finalize())[..16].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contracts::{TaskPriority, TaskStatus};
    use std::collections::HashMap;
    use std::path::PathBuf;

    fn detected_comment(
        path: &str,
        line: usize,
        comment_type: CommentType,
        content: &str,
    ) -> DetectedComment {
        DetectedComment {
            file_path: PathBuf::from(path),
            line_number: line,
            comment_type,
            content: content.to_string(),
            context: "ctx".to_string(),
        }
    }

    fn watch_task(custom_fields: HashMap<String, String>) -> Task {
        Task {
            id: "RQ-0001".to_string(),
            status: TaskStatus::Todo,
            title: "watch task".to_string(),
            description: None,
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
            started_at: None,
            estimated_minutes: None,
            actual_minutes: None,
            scheduled_start: None,
            depends_on: vec![],
            blocks: vec![],
            relates_to: vec![],
            duplicates: None,
            custom_fields,
            parent_id: None,
        }
    }

    #[test]
    fn identity_changes_with_file_and_line() {
        let a = WatchCommentIdentity::from_detected_comment(&detected_comment(
            "/tmp/a.rs",
            10,
            CommentType::Todo,
            "fix this",
        ));
        let b = WatchCommentIdentity::from_detected_comment(&detected_comment(
            "/tmp/b.rs",
            10,
            CommentType::Todo,
            "fix this",
        ));
        let c = WatchCommentIdentity::from_detected_comment(&detected_comment(
            "/tmp/a.rs",
            11,
            CommentType::Todo,
            "fix this",
        ));

        assert_ne!(a.identity_key, b.identity_key);
        assert_ne!(a.identity_key, c.identity_key);
    }

    #[test]
    fn parse_v2_identity_backfills_missing_derived_fields() {
        let mut custom_fields = HashMap::new();
        custom_fields.insert(
            WATCH_FIELD_VERSION.to_string(),
            WATCH_VERSION_V2.to_string(),
        );
        custom_fields.insert(WATCH_FIELD_FILE.to_string(), r"C:\tmp\file.rs".to_string());
        custom_fields.insert(WATCH_FIELD_LINE.to_string(), "42".to_string());
        custom_fields.insert(WATCH_FIELD_COMMENT_TYPE.to_string(), "todo".to_string());
        custom_fields.insert(
            WATCH_FIELD_CONTENT_HASH.to_string(),
            "abcd1234abcd1234".to_string(),
        );

        let parsed = parse_task_watch_identity(&watch_task(custom_fields)).unwrap();
        let ParsedWatchIdentity::V2(identity) = parsed else {
            panic!("expected V2 identity");
        };

        assert_eq!(identity.file, "C:/tmp/file.rs");
        assert_eq!(identity.location_key.len(), 16);
        assert_eq!(identity.identity_key.len(), 16);
    }

    #[test]
    fn parse_legacy_identity_uses_structured_fields_only() {
        let mut custom_fields = HashMap::new();
        custom_fields.insert(WATCH_FIELD_FILE.to_string(), "/tmp/file.rs".to_string());
        custom_fields.insert(WATCH_FIELD_LINE.to_string(), "8".to_string());
        custom_fields.insert(
            WATCH_FIELD_FINGERPRINT.to_string(),
            "abcd1234abcd1234".to_string(),
        );

        let parsed = parse_task_watch_identity(&watch_task(custom_fields)).unwrap();
        assert!(matches!(parsed, ParsedWatchIdentity::LegacyStructured(_)));
    }
}
