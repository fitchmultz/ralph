//! Fuzzy search for tasks with relevance scoring.
//!
//! Purpose:
//! - Fuzzy search for tasks with relevance scoring.
//!
//! Responsibilities:
//! - Search tasks using fuzzy matching with relevance scoring
//! - Return results sorted by score (highest first)
//!
//! Not handled here:
//! - Substring or regex matching (see substring.rs)
//! - Status/tag/scope filtering (see filter.rs)
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Empty/whitespace query returns empty results
//! - Case matching respects the case_sensitive parameter
//! - Best field score per task is used (excludes tasks with score == 0)
//! - Results are sorted by score descending using stable sort
//! - All text fields are searched (title, evidence, plan, notes, request, tags, scope, custom_fields)

use crate::contracts::Task;
use crate::queue::search::fields::for_each_searchable_text;
use anyhow::Result;

/// Search tasks using fuzzy matching with relevance scoring.
///
/// Returns tasks sorted by match score (highest first). Each task is searched
/// across all text fields (title, evidence, plan, notes, request, tags, scope,
/// custom fields). The best matching field's score is used for the task.
pub fn fuzzy_search_tasks<'a>(
    tasks: impl IntoIterator<Item = &'a Task>,
    query: &str,
    case_sensitive: bool,
) -> Result<Vec<(u32, &'a Task)>> {
    use nucleo_matcher::pattern::{CaseMatching, Normalization};
    use nucleo_matcher::{Config, Matcher, Utf32String};

    let query = query.trim();
    if query.is_empty() {
        return Ok(Vec::new());
    }

    let case_matching = if case_sensitive {
        CaseMatching::Respect
    } else {
        CaseMatching::Ignore
    };

    let mut matcher = Matcher::new(Config::DEFAULT);
    let pattern =
        nucleo_matcher::pattern::Pattern::parse(query, case_matching, Normalization::Smart);

    let mut results = Vec::new();

    for task in tasks {
        let mut best_score: u32 = 0;

        // Check all searchable fields
        for_each_searchable_text(task, |text| {
            if text.is_empty() {
                return;
            }
            let haystack: Utf32String = text.into();
            if let Some(score) = pattern.score(haystack.slice(..), &mut matcher)
                && score > best_score
            {
                best_score = score;
            }
        });

        if best_score > 0 {
            results.push((best_score, task));
        }
    }

    // Sort by score descending (highest first)
    results.sort_by(|a, b| b.0.cmp(&a.0));

    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::queue::search::test_support::task;

    #[test]
    fn fuzzy_search_basic_match() -> Result<()> {
        let mut t1 = task("RQ-0001");
        t1.title = "Fix authentication bug".to_string();

        let mut t2 = task("RQ-0002");
        t2.title = "Update documentation".to_string();

        let tasks: Vec<&Task> = vec![&t1, &t2];
        let results = fuzzy_search_tasks(tasks.iter().copied(), "auth bug", false)?;
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].1.id, "RQ-0001");
        Ok(())
    }

    #[test]
    fn fuzzy_search_typo_tolerance() -> Result<()> {
        let mut t1 = task("RQ-0001");
        t1.title = "Implement fuzzy search".to_string();

        let tasks: Vec<&Task> = vec![&t1];
        // Typo: "fzy" should still match "fuzzy"
        let results = fuzzy_search_tasks(tasks.iter().copied(), "fzy srch", false)?;
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].1.id, "RQ-0001");
        Ok(())
    }

    #[test]
    fn fuzzy_search_case_insensitive() -> Result<()> {
        let mut t1 = task("RQ-0001");
        t1.title = "Fix LOGIN Bug".to_string();

        let tasks: Vec<&Task> = vec![&t1];
        let results = fuzzy_search_tasks(tasks.iter().copied(), "login", false)?;
        assert_eq!(results.len(), 1);
        Ok(())
    }

    #[test]
    fn fuzzy_search_case_sensitive() -> Result<()> {
        let mut t1 = task("RQ-0001");
        t1.title = "Fix LOGIN Bug".to_string();

        let tasks: Vec<&Task> = vec![&t1];
        // Case sensitive search for "login" should not match "LOGIN"
        let results = fuzzy_search_tasks(tasks.iter().copied(), "login", true)?;
        assert_eq!(results.len(), 0);

        // Case sensitive search for "LOGIN" should match
        let results = fuzzy_search_tasks(tasks.iter().copied(), "LOGIN", true)?;
        assert_eq!(results.len(), 1);
        Ok(())
    }

    #[test]
    fn fuzzy_search_empty_query_returns_empty() -> Result<()> {
        let t1 = task("RQ-0001");
        let tasks: Vec<&Task> = vec![&t1];
        let results = fuzzy_search_tasks(tasks.iter().copied(), "", false)?;
        assert_eq!(results.len(), 0);
        Ok(())
    }

    #[test]
    fn fuzzy_search_no_match_returns_empty() -> Result<()> {
        let mut t1 = task("RQ-0001");
        t1.title = "Fix authentication".to_string();

        let tasks: Vec<&Task> = vec![&t1];
        let results = fuzzy_search_tasks(tasks.iter().copied(), "xyz123", false)?;
        assert_eq!(results.len(), 0);
        Ok(())
    }

    #[test]
    fn fuzzy_search_scores_sorted() -> Result<()> {
        let mut t1 = task("RQ-0001");
        t1.title = "fuzzy search implementation".to_string();

        let mut t2 = task("RQ-0002");
        t2.title = "something else entirely".to_string();

        let mut t3 = task("RQ-0003");
        t3.title = "fuzzy search and more".to_string();

        let tasks: Vec<&Task> = vec![&t1, &t2, &t3];
        let results = fuzzy_search_tasks(tasks.iter().copied(), "fuzzy search", false)?;

        // Should find 2 matches (t1 and t3)
        assert_eq!(results.len(), 2);

        // Results should be sorted by score descending
        // Exact "fuzzy search" at start of t1 should score higher than t3
        assert!(results[0].0 >= results[1].0);
        Ok(())
    }

    #[test]
    fn fuzzy_search_matches_all_fields() -> Result<()> {
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

        let tasks: Vec<&Task> = vec![&t1];

        // Title match
        let results = fuzzy_search_tasks(tasks.iter().copied(), "authentication", false)?;
        assert_eq!(results.len(), 1);

        // Evidence match
        let results = fuzzy_search_tasks(tasks.iter().copied(), "login fails", false)?;
        assert_eq!(results.len(), 1);

        // Plan match
        let results = fuzzy_search_tasks(tasks.iter().copied(), "debug token", false)?;
        assert_eq!(results.len(), 1);

        // Notes match
        let results = fuzzy_search_tasks(tasks.iter().copied(), "checked logs", false)?;
        assert_eq!(results.len(), 1);

        // Request match
        let results = fuzzy_search_tasks(tasks.iter().copied(), "user request", false)?;
        assert_eq!(results.len(), 1);

        // Tag match
        let results = fuzzy_search_tasks(tasks.iter().copied(), "auth", false)?;
        assert_eq!(results.len(), 1);

        // Scope match
        let results = fuzzy_search_tasks(tasks.iter().copied(), "crates/auth", false)?;
        assert_eq!(results.len(), 1);

        // Custom field key match
        let results = fuzzy_search_tasks(tasks.iter().copied(), "severity", false)?;
        assert_eq!(results.len(), 1);

        // Custom field value match
        let results = fuzzy_search_tasks(tasks.iter().copied(), "high", false)?;
        assert_eq!(results.len(), 1);

        Ok(())
    }
}
