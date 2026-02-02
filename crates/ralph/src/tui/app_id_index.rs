//! ID-to-index cache management for the TUI.
//!
//! Responsibilities:
//! - Maintain a cache mapping task IDs to their indices in the queue.
//! - Invalidate and rebuild the cache when the queue revision changes.
//! - Provide efficient lookup of tasks by ID (case-insensitive).
//!
//! Not handled here:
//! - Queue mutation operations (handled by queue module).
//! - Filtered view management (handled by app_filters module).
//! - Task selection or scrolling logic (handled by app module).
//!
//! Invariants/assumptions:
//! - The cache is lazily rebuilt on access when queue_rev differs from id_to_index_rev.
//! - All ID lookups are case-insensitive.
//! - The cache must be rebuilt after any queue modification that changes task order or IDs.

#![allow(dead_code)]

use std::collections::HashMap;

/// Cache for mapping task IDs to their indices in the queue.
#[derive(Debug)]
pub struct IdIndexCache {
    /// Cached task-id to queue index mapping.
    id_to_index: HashMap<String, usize>,
    /// Revision that the cached id->index map was built from.
    id_to_index_rev: u64,
    /// Queue revision that changes whenever tasks are reordered or mutated.
    queue_rev: u64,
    /// Counter for cache rebuilds (used in tests).
    #[cfg(test)]
    pub rebuilds: usize,
}

impl Default for IdIndexCache {
    fn default() -> Self {
        Self {
            id_to_index: HashMap::new(),
            id_to_index_rev: u64::MAX,
            queue_rev: 0,
            #[cfg(test)]
            rebuilds: 0,
        }
    }
}

impl IdIndexCache {
    /// Create a new empty cache.
    pub fn new() -> Self {
        Self::default()
    }

    /// Update the queue revision, invalidating the cache.
    pub fn bump_queue_rev(&mut self) {
        self.queue_rev = self.queue_rev.wrapping_add(1);
    }

    /// Get the current queue revision.
    pub fn queue_rev(&self) -> u64 {
        self.queue_rev
    }

    /// Ensure the id->index map is up to date.
    ///
    /// Rebuilds the cache if the queue revision has changed since the last build.
    pub fn ensure_id_index_map(&mut self, tasks: &[crate::contracts::Task]) {
        if self.id_to_index_rev == self.queue_rev {
            return;
        }

        self.id_to_index.clear();
        for (idx, task) in tasks.iter().enumerate() {
            self.id_to_index.insert(task.id.clone(), idx);
        }
        self.id_to_index_rev = self.queue_rev;

        #[cfg(test)]
        {
            self.rebuilds += 1;
        }
    }

    /// Look up a task index by its ID (case-sensitive).
    ///
    /// Returns `None` if the ID is not found or if the cache needs rebuilding.
    /// Call `ensure_id_index_map` before this method for accurate results.
    pub fn get(&self, id: &str) -> Option<usize> {
        self.id_to_index.get(id).copied()
    }

    /// Look up a task index by its ID (case-insensitive).
    ///
    /// Returns `None` if no matching ID is found.
    /// Call `ensure_id_index_map` before this method for accurate results.
    pub fn get_case_insensitive(&self, id: &str) -> Option<usize> {
        let normalized_id = id.to_uppercase();
        self.id_to_index
            .iter()
            .find(|(k, _)| k.to_uppercase() == normalized_id)
            .map(|(_, &idx)| idx)
    }

    /// Check if a task ID exists in the cache.
    pub fn contains(&self, id: &str) -> bool {
        self.id_to_index.contains_key(id)
    }

    /// Get the number of entries in the cache.
    pub fn len(&self) -> usize {
        self.id_to_index.len()
    }

    /// Check if the cache is empty.
    pub fn is_empty(&self) -> bool {
        self.id_to_index.is_empty()
    }

    /// Clear the cache and reset the revision tracking.
    pub fn clear(&mut self) {
        self.id_to_index.clear();
        self.id_to_index_rev = u64::MAX;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contracts::Task;

    fn create_task(id: &str) -> Task {
        Task {
            id: id.to_string(),
            title: format!("Task {}", id),
            ..Default::default()
        }
    }

    #[test]
    fn test_cache_builds_correctly() {
        let mut cache = IdIndexCache::new();
        let tasks = vec![create_task("RQ-0001"), create_task("RQ-0002")];

        cache.ensure_id_index_map(&tasks);

        assert_eq!(cache.get("RQ-0001"), Some(0));
        assert_eq!(cache.get("RQ-0002"), Some(1));
        assert_eq!(cache.rebuilds, 1);
    }

    #[test]
    fn test_cache_rebuilds_on_rev_change() {
        let mut cache = IdIndexCache::new();
        let tasks = vec![create_task("RQ-0001")];

        cache.ensure_id_index_map(&tasks);
        assert_eq!(cache.rebuilds, 1);

        // Second call with same rev should not rebuild
        cache.ensure_id_index_map(&tasks);
        assert_eq!(cache.rebuilds, 1);

        // Bump rev and rebuild
        cache.bump_queue_rev();
        cache.ensure_id_index_map(&tasks);
        assert_eq!(cache.rebuilds, 2);
    }

    #[test]
    fn test_case_insensitive_lookup() {
        let mut cache = IdIndexCache::new();
        let tasks = vec![create_task("RQ-0001")];

        cache.ensure_id_index_map(&tasks);

        assert_eq!(cache.get_case_insensitive("rq-0001"), Some(0));
        assert_eq!(cache.get_case_insensitive("RQ-0001"), Some(0));
        assert_eq!(cache.get_case_insensitive("Rq-0001"), Some(0));
    }

    #[test]
    fn test_missing_id() {
        let mut cache = IdIndexCache::new();
        let tasks = vec![create_task("RQ-0001")];

        cache.ensure_id_index_map(&tasks);

        assert_eq!(cache.get("RQ-9999"), None);
        assert_eq!(cache.get_case_insensitive("RQ-9999"), None);
    }
}
