//! Substring and regex search for tasks.
//!
//! Responsibilities:
//! - Search tasks by substring or regex pattern across all text fields
//! - Handle regex compilation with descriptive error messages
//!
//! Not handled here:
//! - Fuzzy matching (see fuzzy.rs)
//! - Status/tag/scope filtering (see filter.rs)
//!
//! Invariants/assumptions:
//! - Empty/whitespace query returns empty results
//! - Regex mode uses RegexBuilder with case_insensitive(!case_sensitive)
//! - Invalid regex returns error with context containing "Invalid regular expression"
//! - All text fields are searched (title, evidence, plan, notes, request, tags, scope, custom_fields)

use crate::contracts::Task;
use crate::queue::search::fields::for_each_searchable_text;
use anyhow::{Context, Result};
use regex::{Regex, RegexBuilder};

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
        let mut matched = false;
        for_each_searchable_text(task, |text| {
            if !matched && matcher.matches(text) {
                matched = true;
            }
        });
        if matched {
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
    use crate::queue::search::test_support::task;

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
}
