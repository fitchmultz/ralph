//! Task queue search and filtering.
//!
//! This module contains logic for:
//! - filtering tasks by status/tag/scope
//! - searching across task text fields (substring or regex)
//!
//! It is split out from `queue.rs` to keep that parent module focused on
//! persistence/repair/validation while keeping a stable public API via
//! re-exports from `crate::queue`.

use crate::contracts::{QueueFile, Task, TaskStatus};
use anyhow::{Context, Result};
use regex::{Regex, RegexBuilder};
use std::collections::HashSet;

/// Options controlling search and filtering behavior.
///
/// This struct unifies the parameters used by both CLI and TUI for
/// consistent search semantics across surfaces.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SearchOptions {
    /// Use regular expression matching (default: false, use substring).
    pub use_regex: bool,
    /// Case-sensitive search (default: false, case-insensitive).
    pub case_sensitive: bool,
    /// Scope filter tokens (default: empty, no scope filter).
    pub scopes: Vec<String>,
}

fn normalize_scope(value: &str) -> String {
    value.trim().to_lowercase()
}

fn normalize_tag(tag: &str) -> String {
    tag.trim().to_lowercase()
}

pub fn filter_tasks<'a>(
    queue: &'a QueueFile,
    statuses: &[TaskStatus],
    tags: &[String],
    scopes: &[String],
    limit: Option<usize>,
) -> Vec<&'a Task> {
    let status_filter: HashSet<TaskStatus> = statuses.iter().copied().collect();
    let tag_filter: HashSet<String> = tags
        .iter()
        .map(|tag| normalize_tag(tag))
        .filter(|tag| !tag.is_empty())
        .collect();
    let scope_filter: Vec<String> = scopes
        .iter()
        .map(|scope| normalize_scope(scope))
        .filter(|scope| !scope.is_empty())
        .collect();

    let has_status_filter = !status_filter.is_empty();
    let has_tag_filter = !tag_filter.is_empty();
    let has_scope_filter = !scope_filter.is_empty();

    let mut out = Vec::new();
    for task in &queue.tasks {
        if has_status_filter && !status_filter.contains(&task.status) {
            continue;
        }
        if has_tag_filter
            && !task
                .tags
                .iter()
                .any(|tag| tag_filter.contains(&normalize_tag(tag)))
        {
            continue;
        }
        if has_scope_filter
            && !task.scope.iter().any(|scope| {
                let hay = normalize_scope(scope);
                scope_filter.iter().any(|needle| hay.contains(needle))
            })
        {
            continue;
        }

        out.push(task);
        if let Some(limit) = limit {
            if out.len() >= limit {
                break;
            }
        }
    }
    out
}

pub fn search_tasks<'a>(
    tasks: impl IntoIterator<Item = &'a Task>,
    query: &str,
    use_regex: bool,
    case_sensitive: bool,
) -> Result<Vec<&'a Task>> {
    let query = query.trim();
    if query.is_empty() {
        return Ok(Vec::new());
    }

    let matcher = if use_regex {
        let regex = RegexBuilder::new(query)
            .case_insensitive(!case_sensitive)
            .build()
            .with_context(|| {
            format!(
                "Invalid regular expression pattern '{}'. Provide a valid regex pattern or use substring search without --regex.",
                query
            )
        })?;
        SearchMatcher::Regex(regex)
    } else {
        let pattern = if case_sensitive {
            query.to_string()
        } else {
            query.to_lowercase()
        };
        SearchMatcher::Substring {
            pattern,
            case_sensitive,
        }
    };

    let mut results = Vec::new();
    for task in tasks {
        if matcher.matches(&task.title)
            || task.evidence.iter().any(|e| matcher.matches(e))
            || task.plan.iter().any(|p| matcher.matches(p))
            || task.notes.iter().any(|n| matcher.matches(n))
            || task.request.as_ref().is_some_and(|r| matcher.matches(r))
            || task.tags.iter().any(|t| matcher.matches(t))
            || task.scope.iter().any(|s| matcher.matches(s))
            || task
                .custom_fields
                .iter()
                .any(|(k, v)| matcher.matches(k) || matcher.matches(v))
        {
            results.push(task);
        }
    }

    Ok(results)
}

enum SearchMatcher {
    Regex(Regex),
    Substring {
        pattern: String,
        case_sensitive: bool,
    },
}

impl SearchMatcher {
    fn matches(&self, text: &str) -> bool {
        let haystack = text.trim();
        if haystack.is_empty() {
            return false;
        }
        match self {
            SearchMatcher::Regex(re) => re.is_match(haystack),
            SearchMatcher::Substring {
                pattern,
                case_sensitive,
            } => {
                if *case_sensitive {
                    haystack.contains(pattern)
                } else {
                    haystack.to_lowercase().contains(pattern)
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contracts::{Task, TaskStatus};
    use std::collections::HashMap;

    fn task(id: &str) -> Task {
        task_with(id, TaskStatus::Todo, vec!["code".to_string()])
    }

    fn task_with(id: &str, status: TaskStatus, tags: Vec<String>) -> Task {
        Task {
            id: id.to_string(),
            status,
            title: "Test task".to_string(),
            priority: Default::default(),
            tags,
            scope: vec!["crates/ralph".to_string()],
            evidence: vec!["observed".to_string()],
            plan: vec!["do thing".to_string()],
            notes: vec![],
            request: Some("test request".to_string()),
            agent: None,
            created_at: Some("2026-01-18T00:00:00Z".to_string()),
            updated_at: Some("2026-01-18T00:00:00Z".to_string()),
            completed_at: None,
            depends_on: vec![],
            custom_fields: HashMap::new(),
        }
    }

    #[test]
    fn search_tasks_substring_case_insensitive() -> Result<()> {
        let mut t1 = task("RQ-0001");
        t1.title = "Fix login bug".to_string();
        t1.evidence = vec!["Users report authentication failure".to_string()];
        t1.plan = vec!["Debug auth service".to_string()];
        t1.notes = vec!["Check token expiration".to_string()];

        let mut t2 = task("RQ-0002");
        t2.title = "Update docs".to_string();
        t2.evidence = vec!["Documentation needs refresh".to_string()];

        let tasks: Vec<&Task> = vec![&t1, &t2];
        let results = search_tasks(tasks, "LOGIN", false, false)?;
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "RQ-0001");
        Ok(())
    }

    #[test]
    fn search_tasks_substring_case_sensitive() -> Result<()> {
        let mut t1 = task("RQ-0001");
        t1.title = "Fix Login bug".to_string();

        let mut t2 = task("RQ-0002");
        t2.title = "Fix login bug".to_string();

        let tasks: Vec<&Task> = vec![&t1, &t2];
        let results = search_tasks(tasks, "Login", false, true)?;
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "RQ-0001");
        Ok(())
    }

    #[test]
    fn search_tasks_regex_valid_pattern() -> Result<()> {
        let mut t1 = task("RQ-0001");
        t1.title = "Fix RQ-1234 bug".to_string();

        let mut t2 = task("RQ-0002");
        t2.title = "Update docs".to_string();

        let tasks: Vec<&Task> = vec![&t1, &t2];
        let results = search_tasks(tasks, r"RQ-\d{4}", true, false)?;
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "RQ-0001");
        Ok(())
    }

    #[test]
    fn search_tasks_regex_invalid_pattern() {
        let t1 = task("RQ-0001");
        let tasks: Vec<&Task> = vec![&t1];
        let err = search_tasks(tasks, r"(?P<unclosed", true, false).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("Invalid regular expression"));
    }

    #[test]
    fn search_tasks_matches_all_fields() -> Result<()> {
        let mut t1 = task("RQ-0001");
        t1.title = "Fix authentication".to_string();
        t1.evidence = vec!["Login fails".to_string()];
        t1.plan = vec!["Debug token".to_string()];
        t1.notes = vec!["Checked logs".to_string()];
        t1.request = Some("User request to fix login".to_string());
        t1.tags = vec!["auth".to_string(), "bug".to_string()];
        t1.scope = vec!["crates/auth".to_string()];
        t1.custom_fields
            .insert("severity".to_string(), "high".to_string());
        t1.custom_fields
            .insert("owner".to_string(), "team-security".to_string());

        let tasks: Vec<&Task> = vec![&t1];

        // Title match
        let results = search_tasks(tasks.iter().copied(), "authentication", false, false)?;
        assert_eq!(results.len(), 1);

        // Evidence match
        let results = search_tasks(tasks.iter().copied(), "login fails", false, false)?;
        assert_eq!(results.len(), 1);

        // Plan match
        let results = search_tasks(tasks.iter().copied(), "debug token", false, false)?;
        assert_eq!(results.len(), 1);

        // Notes match
        let results = search_tasks(tasks.iter().copied(), "checked logs", false, false)?;
        assert_eq!(results.len(), 1);

        // Request match
        let results = search_tasks(tasks.iter().copied(), "user request", false, false)?;
        assert_eq!(results.len(), 1);

        // Tag match
        let results = search_tasks(tasks.iter().copied(), "bug", false, false)?;
        assert_eq!(results.len(), 1);

        // Scope match
        let results = search_tasks(tasks.iter().copied(), "crates/auth", false, false)?;
        assert_eq!(results.len(), 1);

        // Custom field key match
        let results = search_tasks(tasks.iter().copied(), "severity", false, false)?;
        assert_eq!(results.len(), 1);

        // Custom field value match
        let results = search_tasks(tasks.iter().copied(), "team-security", false, false)?;
        assert_eq!(results.len(), 1);

        Ok(())
    }

    #[test]
    fn search_tasks_empty_query_returns_empty() -> Result<()> {
        let t1 = task("RQ-0001");
        let tasks: Vec<&Task> = vec![&t1];
        let results = search_tasks(tasks.iter().copied(), "", false, false)?;
        assert_eq!(results.len(), 0);
        Ok(())
    }

    #[test]
    fn search_tasks_no_match_returns_empty() -> Result<()> {
        let mut t1 = task("RQ-0001");
        t1.title = "Fix authentication".to_string();

        let tasks: Vec<&Task> = vec![&t1];
        let results = search_tasks(tasks.iter().copied(), "database", false, false)?;
        assert_eq!(results.len(), 0);
        Ok(())
    }

    #[test]
    fn search_tasks_regex_case_sensitive_flag() -> Result<()> {
        let mut t1 = task("RQ-0001");
        t1.title = "Fix LOGIN bug".to_string();

        let tasks: Vec<&Task> = vec![&t1];
        let results = search_tasks(tasks.iter().copied(), "LOGIN", true, false)?;
        assert_eq!(results.len(), 1);

        let results = search_tasks(tasks.iter().copied(), "login", true, false)?;
        assert_eq!(results.len(), 1);

        let results = search_tasks(tasks.iter().copied(), "login", true, true)?;
        assert_eq!(results.len(), 0);
        Ok(())
    }

    #[test]
    fn search_options_default_values() {
        let opts = SearchOptions::default();
        assert!(!opts.use_regex, "default: substring search");
        assert!(!opts.case_sensitive, "default: case-insensitive");
        assert!(opts.scopes.is_empty(), "default: no scope filter");
    }

    #[test]
    fn search_options_regex_enabled() {
        let opts = SearchOptions {
            use_regex: true,
            case_sensitive: false,
            scopes: vec![],
        };
        assert!(opts.use_regex, "regex enabled");
        assert!(!opts.case_sensitive, "case-insensitive");
    }

    #[test]
    fn search_options_case_sensitive_enabled() {
        let opts = SearchOptions {
            use_regex: false,
            case_sensitive: true,
            scopes: vec![],
        };
        assert!(!opts.use_regex, "substring search");
        assert!(opts.case_sensitive, "case-sensitive");
    }

    #[test]
    fn search_options_both_enabled() {
        let opts = SearchOptions {
            use_regex: true,
            case_sensitive: true,
            scopes: vec![],
        };
        assert!(opts.use_regex, "regex enabled");
        assert!(opts.case_sensitive, "case-sensitive");
    }

    #[test]
    fn search_options_with_scopes() {
        let opts = SearchOptions {
            use_regex: false,
            case_sensitive: false,
            scopes: vec!["crates/ralph".to_string()],
        };
        assert!(!opts.use_regex, "substring search");
        assert!(!opts.case_sensitive, "case-insensitive");
        assert_eq!(opts.scopes.len(), 1, "one scope filter");
        assert_eq!(opts.scopes[0], "crates/ralph");
    }

    fn task_with_scope(id: &str, scope: Vec<String>) -> Task {
        let mut t = task(id);
        t.scope = scope;
        t
    }

    fn task_with_tags_scope_status(
        id: &str,
        tags: Vec<String>,
        scope: Vec<String>,
        status: TaskStatus,
    ) -> Task {
        let mut t = task(id);
        t.tags = tags;
        t.scope = scope;
        t.status = status;
        t
    }

    #[test]
    fn filter_tasks_with_scope_filter() {
        let queue = QueueFile {
            version: 1,
            tasks: vec![
                task_with_scope("RQ-0001", vec!["crates/ralph".to_string()]),
                task_with_scope("RQ-0002", vec!["docs/cli".to_string()]),
                task_with_scope("RQ-0003", vec!["crates/auth".to_string()]),
            ],
        };

        let results = filter_tasks(&queue, &[], &[], &["crates/ralph".to_string()], None);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "RQ-0001");
    }

    #[test]
    fn filter_tasks_scope_filter_case_insensitive() {
        let queue = QueueFile {
            version: 1,
            tasks: vec![
                task_with_scope("RQ-0001", vec!["CRATES/ralph".to_string()]),
                task_with_scope("RQ-0002", vec!["docs/cli".to_string()]),
            ],
        };

        let results = filter_tasks(&queue, &[], &[], &["crates/ralph".to_string()], None);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "RQ-0001");
    }

    #[test]
    fn filter_tasks_scope_filter_substring() {
        let queue = QueueFile {
            version: 1,
            tasks: vec![
                task_with_scope("RQ-0001", vec!["crates/ralph/src/cli".to_string()]),
                task_with_scope("RQ-0002", vec!["docs/cli".to_string()]),
                task_with_scope("RQ-0003", vec!["crates/auth".to_string()]),
            ],
        };

        let results = filter_tasks(&queue, &[], &[], &["crates/ralph".to_string()], None);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "RQ-0001");
    }

    #[test]
    fn filter_tasks_with_multiple_scopes_or_logic() {
        let queue = QueueFile {
            version: 1,
            tasks: vec![
                task_with_scope("RQ-0001", vec!["crates/ralph".to_string()]),
                task_with_scope("RQ-0002", vec!["docs".to_string()]),
                task_with_scope("RQ-0003", vec!["crates/auth".to_string()]),
            ],
        };

        let results = filter_tasks(
            &queue,
            &[],
            &[],
            &["crates/ralph".to_string(), "docs".to_string()],
            None,
        );
        assert_eq!(results.len(), 2);
        assert!(results.iter().any(|t| t.id == "RQ-0001"));
        assert!(results.iter().any(|t| t.id == "RQ-0002"));
    }

    #[test]
    fn filter_tasks_with_no_scope_filter() {
        let queue = QueueFile {
            version: 1,
            tasks: vec![
                task_with_scope("RQ-0001", vec!["crates/ralph".to_string()]),
                task_with_scope("RQ-0002", vec!["docs/cli".to_string()]),
            ],
        };

        let results = filter_tasks(&queue, &[], &[], &[], None);
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn filter_tasks_combined_filters() {
        let queue = QueueFile {
            version: 1,
            tasks: vec![
                task_with_tags_scope_status(
                    "RQ-0001",
                    vec!["rust".to_string()],
                    vec!["crates/ralph".to_string()],
                    TaskStatus::Todo,
                ),
                task_with_tags_scope_status(
                    "RQ-0002",
                    vec!["docs".to_string()],
                    vec!["docs".to_string()],
                    TaskStatus::Done,
                ),
                task_with_tags_scope_status(
                    "RQ-0003",
                    vec!["rust".to_string()],
                    vec!["crates".to_string()],
                    TaskStatus::Doing,
                ),
                task_with_tags_scope_status(
                    "RQ-0004",
                    vec!["rust".to_string()],
                    vec!["crates/ralph".to_string()],
                    TaskStatus::Todo,
                ),
            ],
        };

        let results = filter_tasks(
            &queue,
            &[TaskStatus::Todo],
            &["rust".to_string()],
            &["crates/ralph".to_string()],
            None,
        );
        assert_eq!(results.len(), 2);
        assert!(results.iter().any(|t| t.id == "RQ-0001"));
        assert!(results.iter().any(|t| t.id == "RQ-0004"));
    }

    #[test]
    fn filter_tasks_status_only() {
        let queue = QueueFile {
            version: 1,
            tasks: vec![
                task_with_tags_scope_status("RQ-0001", vec![], vec![], TaskStatus::Todo),
                task_with_tags_scope_status("RQ-0002", vec![], vec![], TaskStatus::Doing),
                task_with_tags_scope_status("RQ-0003", vec![], vec![], TaskStatus::Todo),
            ],
        };

        let results = filter_tasks(&queue, &[TaskStatus::Todo], &[], &[], None);
        assert_eq!(results.len(), 2);
        assert!(results.iter().all(|t| t.status == TaskStatus::Todo));
    }

    #[test]
    fn filter_tasks_tag_only() {
        let queue = QueueFile {
            version: 1,
            tasks: vec![
                task_with_tags_scope_status(
                    "RQ-0001",
                    vec!["rust".to_string()],
                    vec![],
                    TaskStatus::Todo,
                ),
                task_with_tags_scope_status(
                    "RQ-0002",
                    vec!["docs".to_string()],
                    vec![],
                    TaskStatus::Todo,
                ),
                task_with_tags_scope_status(
                    "RQ-0003",
                    vec!["RUST".to_string()],
                    vec![],
                    TaskStatus::Doing,
                ),
            ],
        };

        let results = filter_tasks(&queue, &[], &["rust".to_string()], &[], None);
        assert_eq!(results.len(), 2);
        assert!(results.iter().any(|t| t.id == "RQ-0001"));
        assert!(results.iter().any(|t| t.id == "RQ-0003"));
    }

    #[test]
    fn filter_tasks_with_limit() {
        let queue = QueueFile {
            version: 1,
            tasks: vec![
                task_with_tags_scope_status(
                    "RQ-0001",
                    vec!["rust".to_string()],
                    vec!["crates/ralph".to_string()],
                    TaskStatus::Todo,
                ),
                task_with_tags_scope_status(
                    "RQ-0002",
                    vec!["rust".to_string()],
                    vec!["crates/ralph".to_string()],
                    TaskStatus::Todo,
                ),
                task_with_tags_scope_status(
                    "RQ-0003",
                    vec!["rust".to_string()],
                    vec!["crates/ralph".to_string()],
                    TaskStatus::Todo,
                ),
            ],
        };

        let results = filter_tasks(
            &queue,
            &[TaskStatus::Todo],
            &["rust".to_string()],
            &["crates/ralph".to_string()],
            Some(2),
        );
        assert_eq!(results.len(), 2);
    }
}
