//! Task queue search and filtering.
//!
//! Responsibilities:
//! - Filtering tasks by status/tag/scope
//! - Searching across task text fields (substring, regex, or fuzzy matching)
//!
//! Not handled here:
//! - Queue persistence, repair, or validation (see `crate::queue`)
//! - Task mutation or state changes
//! - Search result ordering beyond basic filtering
//!
//! Invariants/assumptions:
//! - Search patterns are normalized (lowercase, trimmed) before comparison
//! - Regex compilation failures are propagated to callers
//! - Empty filter sets match all tasks (no filtering applied)
//!
//! It is split out from `queue.rs` to keep that parent module focused on
//! persistence/repair/validation while keeping a stable public API via
//! re-exports from `crate::queue`.

mod fields;
mod filter;
mod fuzzy;
mod normalize;
mod options;
mod substring;

#[cfg(test)]
mod test_support;

pub use filter::filter_tasks;
pub use fuzzy::fuzzy_search_tasks;
pub use options::SearchOptions;
pub use substring::search_tasks;

use crate::contracts::Task;
use anyhow::Result;

/// Unified search entry point that handles all search modes.
///
/// Delegates to fuzzy matching, regex, or substring search based on
/// the options provided. Fuzzy and regex modes are mutually exclusive;
/// fuzzy takes precedence if both are enabled.
pub fn search_tasks_with_options<'a>(
    tasks: impl IntoIterator<Item = &'a Task>,
    query: &str,
    options: &SearchOptions,
) -> Result<Vec<&'a Task>> {
    if options.use_fuzzy {
        fuzzy_search_tasks(tasks, query, options.case_sensitive)
            .map(|results| results.into_iter().map(|(_, task)| task).collect())
    } else {
        search_tasks(tasks, query, options.use_regex, options.case_sensitive)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use test_support::task;

    #[test]
    fn search_tasks_with_options_fuzzy_mode() -> Result<()> {
        let mut t1 = task("RQ-0001");
        t1.title = "Fix authentication".to_string();

        let mut t2 = task("RQ-0002");
        t2.title = "Update docs".to_string();

        let tasks: Vec<&Task> = vec![&t1, &t2];
        let options = SearchOptions {
            use_regex: false,
            case_sensitive: false,
            use_fuzzy: true,
            scopes: vec![],
        };

        let results = search_tasks_with_options(tasks.iter().copied(), "auth", &options)?;
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "RQ-0001");
        Ok(())
    }

    #[test]
    fn search_tasks_with_options_regex_mode() -> Result<()> {
        let mut t1 = task("RQ-0001");
        t1.title = "Fix RQ-1234 bug".to_string();

        let mut t2 = task("RQ-0002");
        t2.title = "Update docs".to_string();

        let tasks: Vec<&Task> = vec![&t1, &t2];
        let options = SearchOptions {
            use_regex: true,
            case_sensitive: false,
            use_fuzzy: false,
            scopes: vec![],
        };

        let results = search_tasks_with_options(tasks.iter().copied(), r"RQ-\d{4}", &options)?;
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "RQ-0001");
        Ok(())
    }

    #[test]
    fn search_tasks_with_options_substring_mode() -> Result<()> {
        let mut t1 = task("RQ-0001");
        t1.title = "Fix authentication".to_string();

        let mut t2 = task("RQ-0002");
        t2.title = "Update docs".to_string();

        let tasks: Vec<&Task> = vec![&t1, &t2];
        let options = SearchOptions {
            use_regex: false,
            case_sensitive: false,
            use_fuzzy: false,
            scopes: vec![],
        };

        let results = search_tasks_with_options(tasks.iter().copied(), "auth", &options)?;
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "RQ-0001");
        Ok(())
    }
}
