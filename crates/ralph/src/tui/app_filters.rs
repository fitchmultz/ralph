//! Filter state management and cache for the TUI.
//!
//! Responsibilities:
//! - Store active filter state (query, tags, statuses, scopes, search options).
//! - Maintain a cache of filtered task indices for efficient rendering.
//! - Provide methods for rebuilding the filtered view and managing filter snapshots.
//! - Handle filter toggles (case-sensitive, regex, fuzzy) and input modes.
//!
//! Not handled here:
//! - Queue mutation operations (handled by queue module).
//! - Task selection logic (handled by app module).
//! - Rendering of filter UI (handled by render module).
//!
//! Invariants/assumptions:
//! - The filtered indices cache is invalidated when queue_rev or filters change.
//! - Filter snapshots are used to restore state when canceling filter input.
//! - All filter operations preserve the selected task when possible.

use crate::contracts::{QueueFile, TaskStatus};
use crate::queue::{self, SearchOptions};

/// Active filters applied to the task list.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FilterState {
    /// Free-text search query (substring match across task fields).
    pub query: String,
    /// Status filters (empty means all statuses).
    pub statuses: Vec<TaskStatus>,
    /// Tag filters (empty means all tags).
    pub tags: Vec<String>,
    /// Search options (regex mode, case sensitivity).
    pub search_options: SearchOptions,
}

/// Snapshot of filters before entering a live filter input mode.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FilterSnapshot {
    pub filters: FilterState,
    pub selected_task_id: Option<String>,
}

/// Cache key for filtered indices to detect when rebuild is needed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FilterKey {
    query: String,
    statuses: Vec<TaskStatus>,
    tags: Vec<String>,
    scopes: Vec<String>,
    use_regex: bool,
    case_sensitive: bool,
    use_fuzzy: bool,
}

impl FilterKey {
    /// Create a filter key from the current filter state.
    pub fn from_filters(filters: &FilterState) -> Self {
        let mut statuses = filters.statuses.clone();
        statuses.sort_by_key(|status| status.as_str());

        let mut tags: Vec<String> = filters
            .tags
            .iter()
            .map(|tag| normalize_filter_token(tag))
            .filter(|tag| !tag.is_empty())
            .collect();
        tags.sort();

        let mut scopes: Vec<String> = filters
            .search_options
            .scopes
            .iter()
            .map(|scope| normalize_filter_token(scope))
            .filter(|scope| !scope.is_empty())
            .collect();
        scopes.sort();

        Self {
            query: filters.query.trim().to_string(),
            statuses,
            tags,
            scopes,
            use_regex: filters.search_options.use_regex,
            case_sensitive: filters.search_options.case_sensitive,
            use_fuzzy: filters.search_options.use_fuzzy,
        }
    }
}

/// Manages filter state and cached filtered indices.
#[derive(Debug)]
pub struct FilterManager {
    /// Active filters applied to the task list.
    pub filters: FilterState,
    /// Snapshot of filters before entering a live filter input mode.
    snapshot: Option<FilterSnapshot>,
    /// Cached filtered task indices into the queue.
    pub filtered_indices: Vec<usize>,
    /// Filter key used for the cached filtered indices.
    last_filter_key: Option<FilterKey>,
    /// Revision that the cached filtered indices were built from.
    filtered_indices_rev: u64,
    /// Queue revision that changes whenever tasks are reordered or mutated.
    queue_rev: u64,
    /// Counter for filtered view rebuilds (used in tests).
    #[cfg(test)]
    pub filtered_rebuilds: usize,
}

impl Default for FilterManager {
    fn default() -> Self {
        Self {
            filters: FilterState::default(),
            snapshot: None,
            filtered_indices: Vec::new(),
            last_filter_key: None,
            filtered_indices_rev: u64::MAX,
            queue_rev: 0,
            #[cfg(test)]
            filtered_rebuilds: 0,
        }
    }
}

impl FilterManager {
    /// Create a new filter manager with default values.
    pub fn new() -> Self {
        Self::default()
    }

    /// Update the queue revision, invalidating the filtered indices cache.
    pub fn bump_queue_rev(&mut self) {
        self.queue_rev = self.queue_rev.wrapping_add(1);
    }

    /// Get the current queue revision.
    pub fn queue_rev(&self) -> u64 {
        self.queue_rev
    }

    /// Ensure the filtered indices are up to date.
    ///
    /// Rebuilds the cache if the queue revision or filters have changed.
    /// Returns true if a rebuild occurred.
    pub fn ensure_filtered_indices(
        &mut self,
        queue: &QueueFile,
        id_to_index: &std::collections::HashMap<String, usize>,
    ) -> bool {
        let filter_key = FilterKey::from_filters(&self.filters);

        if self.filtered_indices_rev == self.queue_rev
            && self.last_filter_key.as_ref() == Some(&filter_key)
        {
            return false;
        }

        let filtered_ids: Vec<String> = {
            let mut filtered = queue::filter_tasks(
                queue,
                &self.filters.statuses,
                &self.filters.tags,
                &self.filters.search_options.scopes,
                None,
            );

            if !self.filters.query.trim().is_empty() {
                match queue::search_tasks_with_options(
                    filtered,
                    &self.filters.query,
                    &self.filters.search_options,
                ) {
                    Ok(results) => {
                        filtered = results;
                    }
                    Err(_) => {
                        // On search error, return empty results
                        filtered = Vec::new();
                    }
                }
            }

            filtered.into_iter().map(|task| task.id.clone()).collect()
        };

        self.filtered_indices = filtered_ids
            .iter()
            .filter_map(|id| id_to_index.get(id).copied())
            .collect();
        self.last_filter_key = Some(filter_key);
        self.filtered_indices_rev = self.queue_rev;

        #[cfg(test)]
        {
            self.filtered_rebuilds += 1;
        }

        true
    }

    /// Return true if any filters are active.
    pub fn has_active_filters(&self) -> bool {
        !self.filters.query.trim().is_empty()
            || !self.filters.tags.is_empty()
            || !self.filters.statuses.is_empty()
            || !self.filters.search_options.scopes.is_empty()
            || self.filters.search_options.use_regex
            || self.filters.search_options.case_sensitive
            || self.filters.search_options.use_fuzzy
    }

    /// Create a human-readable summary of active filters.
    pub fn filter_summary(&self) -> Option<String> {
        if !self.has_active_filters() {
            return None;
        }

        let mut parts = Vec::new();
        if let Some(status) = self.filters.statuses.first() {
            parts.push(format!("status={}", status.as_str()));
        }
        if !self.filters.tags.is_empty() {
            parts.push(format!("tags={}", self.filters.tags.join(",")));
        }
        if !self.filters.query.trim().is_empty() {
            parts.push(format!("query={}", self.filters.query.trim()));
        }
        for scope in &self.filters.search_options.scopes {
            parts.push(format!("scope={}", scope));
        }
        if self.filters.search_options.use_regex {
            parts.push("regex".to_string());
        }
        if self.filters.search_options.use_fuzzy {
            parts.push("fuzzy".to_string());
        }
        if self.filters.search_options.case_sensitive {
            parts.push("case-sensitive".to_string());
        }
        Some(format!("filters: {}", parts.join(" ")))
    }

    /// Toggle case-sensitive search.
    pub fn toggle_case_sensitive(&mut self) -> &str {
        self.filters.search_options.case_sensitive = !self.filters.search_options.case_sensitive;
        if self.filters.search_options.case_sensitive {
            "enabled"
        } else {
            "disabled"
        }
    }

    /// Toggle regex search.
    pub fn toggle_regex(&mut self) -> &str {
        self.filters.search_options.use_regex = !self.filters.search_options.use_regex;
        // Regex and fuzzy are mutually exclusive
        if self.filters.search_options.use_regex && self.filters.search_options.use_fuzzy {
            self.filters.search_options.use_fuzzy = false;
        }
        if self.filters.search_options.use_regex {
            "enabled (fuzzy disabled)"
        } else {
            "disabled"
        }
    }

    /// Toggle fuzzy search.
    pub fn toggle_fuzzy(&mut self) -> &str {
        self.filters.search_options.use_fuzzy = !self.filters.search_options.use_fuzzy;
        // Fuzzy and regex are mutually exclusive
        if self.filters.search_options.use_fuzzy && self.filters.search_options.use_regex {
            self.filters.search_options.use_regex = false;
        }
        if self.filters.search_options.use_fuzzy {
            "enabled (regex disabled)"
        } else {
            "disabled"
        }
    }

    /// Begin filter input mode, saving a snapshot of current state.
    pub fn begin_input(&mut self, selected_task_id: Option<String>) {
        if self.snapshot.is_some() {
            return;
        }
        self.snapshot = Some(FilterSnapshot {
            filters: self.filters.clone(),
            selected_task_id,
        });
    }

    /// Commit the current filter input, clearing the snapshot.
    pub fn commit_input(&mut self) {
        self.snapshot = None;
    }

    /// Restore the filter snapshot from before input mode.
    ///
    /// Returns the previously selected task ID if one was saved.
    pub fn restore_snapshot(&mut self) -> Option<String> {
        let snapshot = self.snapshot.take()?;
        self.filters = snapshot.filters;
        snapshot.selected_task_id
    }

    /// Clear all active filters.
    ///
    /// Returns true if filters were cleared (i.e., they were active before).
    pub fn clear(&mut self) -> bool {
        let had_filters = self.has_active_filters();
        self.filters = FilterState::default();
        had_filters
    }

    /// Set the search query.
    pub fn set_query(&mut self, query: String) {
        self.filters.query = query;
    }

    /// Set the tag filters.
    pub fn set_tags(&mut self, tags: Vec<String>) {
        self.filters.tags = tags;
    }

    /// Set the scope filters.
    pub fn set_scopes(&mut self, scopes: Vec<String>) {
        self.filters.search_options.scopes = scopes;
    }

    /// Set the status filters.
    pub fn set_statuses(&mut self, statuses: Vec<TaskStatus>) {
        self.filters.statuses = statuses;
    }

    /// Get the number of filtered tasks.
    pub fn len(&self) -> usize {
        self.filtered_indices.len()
    }

    /// Check if there are no filtered tasks.
    pub fn is_empty(&self) -> bool {
        self.filtered_indices.is_empty()
    }

    /// Get the filtered task index at the given position.
    pub fn get_index(&self, position: usize) -> Option<usize> {
        self.filtered_indices.get(position).copied()
    }

    /// Find the position of a task ID in the filtered indices.
    pub fn position_of(&self, id: &str, queue: &QueueFile) -> Option<usize> {
        self.filtered_indices
            .iter()
            .enumerate()
            .find_map(|(pos, &idx)| {
                queue
                    .tasks
                    .get(idx)
                    .and_then(|task| if task.id == id { Some(pos) } else { None })
            })
    }
}

/// Normalize a filter token for consistent cache hits.
pub fn normalize_filter_token(value: &str) -> String {
    value.trim().to_lowercase()
}

/// Parse comma or newline-separated list from input string.
#[allow(dead_code)]
pub fn parse_list(input: &str) -> Vec<String> {
    input
        .split([',', '\n'])
        .map(|item| item.trim().to_string())
        .filter(|item| !item.is_empty())
        .collect()
}

/// Parse comma or whitespace-separated tags from input string.
#[allow(dead_code)]
pub fn parse_tags(input: &str) -> Vec<String> {
    input
        .split(|c: char| c == ',' || c.is_whitespace())
        .map(|tag| tag.trim().to_string())
        .filter(|tag| !tag.is_empty())
        .collect()
}

// ============================================================================
// FilterOperations trait for App
// ============================================================================

use crate::tui::App;

/// Trait for filter-related operations on the App.
pub trait FilterOperations {
    /// Toggle case-sensitive search.
    fn toggle_case_sensitive(&mut self);

    /// Toggle regex search.
    fn toggle_regex(&mut self);

    /// Toggle fuzzy search.
    fn toggle_fuzzy(&mut self);
}

impl FilterOperations for App {
    fn toggle_case_sensitive(&mut self) {
        self.filters.search_options.case_sensitive = !self.filters.search_options.case_sensitive;
        let state = if self.filters.search_options.case_sensitive {
            "enabled"
        } else {
            "disabled"
        };
        self.set_status_message(format!("Case-sensitive search {}", state));
        self.rebuild_filtered_view();
    }

    fn toggle_regex(&mut self) {
        self.filters.search_options.use_regex = !self.filters.search_options.use_regex;
        // Regex and fuzzy are mutually exclusive
        if self.filters.search_options.use_regex && self.filters.search_options.use_fuzzy {
            self.filters.search_options.use_fuzzy = false;
        }
        let state = if self.filters.search_options.use_regex {
            "enabled (fuzzy disabled)"
        } else {
            "disabled"
        };
        self.set_status_message(format!("Regex search {}", state));
        self.rebuild_filtered_view();
    }

    fn toggle_fuzzy(&mut self) {
        self.filters.search_options.use_fuzzy = !self.filters.search_options.use_fuzzy;
        // Fuzzy and regex are mutually exclusive
        if self.filters.search_options.use_fuzzy && self.filters.search_options.use_regex {
            self.filters.search_options.use_regex = false;
        }
        let state = if self.filters.search_options.use_fuzzy {
            "enabled (regex disabled)"
        } else {
            "disabled"
        };
        self.set_status_message(format!("Fuzzy search {}", state));
        self.rebuild_filtered_view();
    }
}

// ============================================================================
// FilterManagementOperations trait for App
// ============================================================================

/// Trait for filter management operations.
pub trait FilterManagementOperations {
    /// Cycle the active status filter.
    fn cycle_status_filter(&mut self);

    /// Set the tag filter list (empty to clear).
    fn set_tag_filters(&mut self, tags: Vec<String>);

    /// Set the scope filter list (empty to clear).
    fn set_scope_filters(&mut self, scopes: Vec<String>);

    /// Set the search query (empty to clear).
    fn set_search_query(&mut self, query: String);

    /// Clear all active filters.
    fn clear_filters(&mut self);
}

impl FilterManagementOperations for App {
    fn cycle_status_filter(&mut self) {
        let preferred_id = self.selected_task().map(|t| t.id.clone());
        let next = match self.filters.statuses.as_slice() {
            [] => Some(TaskStatus::Todo),
            [TaskStatus::Todo] => Some(TaskStatus::Doing),
            [TaskStatus::Doing] => Some(TaskStatus::Done),
            [TaskStatus::Done] => Some(TaskStatus::Draft),
            [TaskStatus::Draft] => Some(TaskStatus::Rejected),
            [TaskStatus::Rejected] => None,
            _ => None,
        };

        self.filters.statuses = next.map(|status| vec![status]).unwrap_or_default();
        self.rebuild_filtered_view_with_preferred(preferred_id.as_deref());
    }

    fn set_tag_filters(&mut self, tags: Vec<String>) {
        let preferred_id = self.selected_task().map(|t| t.id.clone());
        self.filters.tags = tags;
        self.rebuild_filtered_view_with_preferred(preferred_id.as_deref());
    }

    fn set_scope_filters(&mut self, scopes: Vec<String>) {
        let preferred_id = self.selected_task().map(|t| t.id.clone());
        self.filters.search_options.scopes = scopes;
        self.rebuild_filtered_view_with_preferred(preferred_id.as_deref());
    }

    fn set_search_query(&mut self, query: String) {
        let preferred_id = self.selected_task().map(|t| t.id.clone());
        self.filters.query = query;
        self.rebuild_filtered_view_with_preferred(preferred_id.as_deref());
    }

    fn clear_filters(&mut self) {
        let preferred_id = self.selected_task().map(|t| t.id.clone());
        self.filters = FilterState::default();
        self.rebuild_filtered_view_with_preferred(preferred_id.as_deref());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contracts::Task;

    #[allow(dead_code)]
    fn create_task(id: &str, title: &str) -> Task {
        Task {
            id: id.to_string(),
            title: title.to_string(),
            ..Default::default()
        }
    }

    #[test]
    fn test_filter_key_from_filters() {
        let filters = FilterState {
            query: "test".to_string(),
            tags: vec!["bug".to_string(), "urgent".to_string()],
            statuses: vec![TaskStatus::Todo],
            ..Default::default()
        };

        let key = FilterKey::from_filters(&filters);

        assert_eq!(key.query, "test");
        assert_eq!(key.tags, vec!["bug", "urgent"]);
        assert_eq!(key.statuses, vec![TaskStatus::Todo]);
    }

    #[test]
    fn test_has_active_filters() {
        let mut manager = FilterManager::new();
        assert!(!manager.has_active_filters());

        manager.filters.query = "test".to_string();
        assert!(manager.has_active_filters());

        manager.clear();
        assert!(!manager.has_active_filters());

        manager.filters.search_options.use_regex = true;
        assert!(manager.has_active_filters());
    }

    #[test]
    fn test_filter_summary() {
        let mut manager = FilterManager::new();
        assert_eq!(manager.filter_summary(), None);

        manager.filters.query = "test".to_string();
        manager.filters.tags = vec!["bug".to_string()];

        let summary = manager.filter_summary().unwrap();
        assert!(summary.contains("query=test"));
        assert!(summary.contains("tags=bug"));
    }

    #[test]
    fn test_snapshot_restore() {
        let mut manager = FilterManager::new();
        manager.filters.query = "original".to_string();

        manager.begin_input(Some("task-1".to_string()));
        manager.filters.query = "modified".to_string();

        let restored_id = manager.restore_snapshot();
        assert_eq!(restored_id, Some("task-1".to_string()));
        assert_eq!(manager.filters.query, "original");
    }

    #[test]
    fn test_normalize_filter_token() {
        assert_eq!(normalize_filter_token("  TEST  "), "test");
        assert_eq!(normalize_filter_token("MixedCase"), "mixedcase");
    }

    #[test]
    fn test_parse_list() {
        let result = parse_list("a, b, c");
        assert_eq!(result, vec!["a", "b", "c"]);

        let result = parse_list("a\nb\nc");
        assert_eq!(result, vec!["a", "b", "c"]);
    }

    #[test]
    fn test_parse_tags() {
        let result = parse_tags("tag1, tag2 tag3");
        assert_eq!(result, vec!["tag1", "tag2", "tag3"]);
    }
}
