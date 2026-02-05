//! Dependency graph cache for TUI performance optimization.
//!
//! Responsibilities:
//! - Cache dependency graph and critical paths to avoid rebuilding on every render.
//! - Provide cache invalidation based on queue/done revision counters.
//! - Track cache statistics for testing and debugging.
//!
//! Not handled here:
//! - Graph construction (delegated to `crate::queue::graph`).
//! - Queue mutations (caller must bump revision counters).
//! - Rendering (cache provides data to renderers).
//!
//! Invariants/assumptions:
//! - The cached graph is valid only when queue_rev and done_rev match.
//! - Critical paths are computed lazily and cached until revisions change.

use crate::contracts::QueueFile;
use crate::queue::graph::{CriticalPathResult, DependencyGraph, build_graph, find_critical_paths};

/// Cache for dependency graph and critical paths to optimize TUI rendering.
///
/// This cache stores the computed dependency graph and critical paths,
/// invalidating them when the queue or done archive changes (tracked via
/// revision counters).
#[derive(Debug, Clone)]
pub struct DependencyGraphCache {
    /// Queue revision when graph was last built.
    queue_rev: u64,
    /// Done revision when graph was last built.
    done_rev: u64,
    /// Cached dependency graph (None when invalidated).
    graph: Option<DependencyGraph>,
    /// Cached critical paths (None when invalidated or not yet computed).
    critical_paths: Option<Vec<CriticalPathResult>>,
    /// Statistics for testing: number of graph builds.
    #[cfg(test)]
    graph_builds: usize,
    /// Statistics for testing: number of critical path computations.
    #[cfg(test)]
    critical_builds: usize,
}

impl Default for DependencyGraphCache {
    fn default() -> Self {
        Self::new()
    }
}

impl DependencyGraphCache {
    /// Create a new empty cache.
    pub fn new() -> Self {
        Self {
            queue_rev: u64::MAX,
            done_rev: u64::MAX,
            graph: None,
            critical_paths: None,
            #[cfg(test)]
            graph_builds: 0,
            #[cfg(test)]
            critical_builds: 0,
        }
    }

    /// Get the cached graph, rebuilding if necessary.
    ///
    /// If the cached graph is invalid (revision mismatch), rebuilds from
    /// the provided queue files. Also clears cached critical paths on rebuild.
    ///
    /// # Arguments
    /// * `queue_rev` - Current queue revision
    /// * `done_rev` - Current done revision
    /// * `active` - Active queue file
    /// * `done` - Done archive queue file
    ///
    /// # Returns
    /// Reference to the cached (or freshly built) dependency graph.
    pub fn graph(
        &mut self,
        queue_rev: u64,
        done_rev: u64,
        active: &QueueFile,
        done: &QueueFile,
    ) -> &DependencyGraph {
        if self.queue_rev != queue_rev || self.done_rev != done_rev || self.graph.is_none() {
            self.graph = Some(build_graph(active, Some(done)));
            self.critical_paths = None; // Invalidate critical paths on graph rebuild
            self.queue_rev = queue_rev;
            self.done_rev = done_rev;
            #[cfg(test)]
            {
                self.graph_builds += 1;
            }
        }
        self.graph.as_ref().unwrap()
    }

    /// Get critical paths, computing if necessary.
    ///
    /// If the cached graph is invalid, rebuilds it first.
    /// If critical paths haven't been computed for the current graph,
    /// computes and caches them.
    ///
    /// # Arguments
    /// * `queue_rev` - Current queue revision
    /// * `done_rev` - Current done revision
    /// * `active` - Active queue file
    /// * `done` - Done archive queue file
    ///
    /// # Returns
    /// Slice of critical path results (may be empty if graph is empty).
    pub fn critical_paths(
        &mut self,
        queue_rev: u64,
        done_rev: u64,
        active: &QueueFile,
        done: &QueueFile,
    ) -> &[CriticalPathResult] {
        // Ensure graph is built first
        let _ = self.graph(queue_rev, done_rev, active, done);

        // Compute critical paths if not cached
        if self.critical_paths.is_none() {
            let paths = self
                .graph
                .as_ref()
                .map(find_critical_paths)
                .unwrap_or_default();
            self.critical_paths = Some(paths);
            #[cfg(test)]
            {
                self.critical_builds += 1;
            }
        }

        self.critical_paths.as_deref().unwrap_or(&[])
    }

    /// Invalidate the cache.
    ///
    /// Marks the cache as invalid, forcing a rebuild on next access.
    /// Call this when you know the queue state has changed but haven't
    /// updated revision counters yet.
    pub fn invalidate(&mut self) {
        self.queue_rev = u64::MAX;
        self.done_rev = u64::MAX;
        self.graph = None;
        self.critical_paths = None;
    }

    /// Check if the cache is valid for the given revisions.
    ///
    /// # Arguments
    /// * `queue_rev` - Current queue revision
    /// * `done_rev` - Current done revision
    ///
    /// # Returns
    /// True if the graph is cached and matches both revisions.
    pub fn is_valid(&self, queue_rev: u64, done_rev: u64) -> bool {
        self.queue_rev == queue_rev && self.done_rev == done_rev && self.graph.is_some()
    }

    /// Get the number of graph builds (test builds only).
    #[cfg(test)]
    pub fn graph_builds(&self) -> usize {
        self.graph_builds
    }

    /// Get the number of critical path computations (test builds only).
    #[cfg(test)]
    pub fn critical_builds(&self) -> usize {
        self.critical_builds
    }

    /// Reset statistics counters (test builds only).
    #[cfg(test)]
    pub fn reset_stats(&mut self) {
        self.graph_builds = 0;
        self.critical_builds = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contracts::{QueueFile, Task, TaskStatus};
    use std::collections::HashMap;

    fn task(id: &str, depends_on: Vec<&str>, status: TaskStatus) -> Task {
        Task {
            id: id.to_string(),
            status,
            title: format!("Task {}", id),
            priority: Default::default(),
            tags: vec!["test".to_string()],
            scope: vec!["test".to_string()],
            evidence: vec!["test".to_string()],
            plan: vec!["test".to_string()],
            notes: vec![],
            request: Some("test".to_string()),
            agent: None,
            created_at: Some("2026-01-18T00:00:00Z".to_string()),
            updated_at: Some("2026-01-18T00:00:00Z".to_string()),
            completed_at: None,
            started_at: None,
            scheduled_start: None,
            depends_on: depends_on.into_iter().map(|s| s.to_string()).collect(),
            blocks: vec![],
            relates_to: vec![],
            duplicates: None,
            custom_fields: HashMap::new(),
            parent_id: None,
        }
    }

    fn queue_file(tasks: Vec<Task>) -> QueueFile {
        QueueFile { version: 1, tasks }
    }

    fn empty_done() -> QueueFile {
        QueueFile {
            version: 1,
            tasks: vec![],
        }
    }

    #[test]
    fn cache_builds_graph_on_first_access() {
        let active = queue_file(vec![task("RQ-0001", vec![], TaskStatus::Todo)]);
        let done = empty_done();
        let mut cache = DependencyGraphCache::new();

        let graph = cache.graph(0, 0, &active, &done);

        assert_eq!(graph.len(), 1);
        assert!(graph.contains("RQ-0001"));
        assert_eq!(cache.graph_builds(), 1);
    }

    #[test]
    fn cache_reuses_graph_on_same_revisions() {
        let active = queue_file(vec![task("RQ-0001", vec![], TaskStatus::Todo)]);
        let done = empty_done();
        let mut cache = DependencyGraphCache::new();

        // First access
        let _ = cache.graph(0, 0, &active, &done);
        assert_eq!(cache.graph_builds(), 1);

        // Second access with same revisions
        let _ = cache.graph(0, 0, &active, &done);
        assert_eq!(cache.graph_builds(), 1); // No rebuild
    }

    #[test]
    fn cache_rebuilds_on_queue_rev_change() {
        let active = queue_file(vec![task("RQ-0001", vec![], TaskStatus::Todo)]);
        let done = empty_done();
        let mut cache = DependencyGraphCache::new();

        // First access
        let _ = cache.graph(0, 0, &active, &done);
        assert_eq!(cache.graph_builds(), 1);

        // Access with different queue revision
        let _ = cache.graph(1, 0, &active, &done);
        assert_eq!(cache.graph_builds(), 2); // Rebuilt
    }

    #[test]
    fn cache_rebuilds_on_done_rev_change() {
        let active = queue_file(vec![task("RQ-0001", vec![], TaskStatus::Todo)]);
        let done = queue_file(vec![task("RQ-0000", vec![], TaskStatus::Done)]);
        let mut cache = DependencyGraphCache::new();

        // First access
        let _ = cache.graph(0, 0, &active, &done);
        assert_eq!(cache.graph_builds(), 1);

        // Access with different done revision
        let _ = cache.graph(0, 1, &active, &done);
        assert_eq!(cache.graph_builds(), 2); // Rebuilt
    }

    #[test]
    fn cache_computes_critical_paths_lazily() {
        let active = queue_file(vec![
            task("RQ-0001", vec![], TaskStatus::Todo),
            task("RQ-0002", vec!["RQ-0001"], TaskStatus::Todo),
        ]);
        let done = empty_done();
        let mut cache = DependencyGraphCache::new();

        // Access graph first (should not compute critical paths)
        let _ = cache.graph(0, 0, &active, &done);
        assert_eq!(cache.critical_builds(), 0);

        // Now access critical paths - need mutable borrow for the whole operation
        let paths_len = {
            let paths = cache.critical_paths(0, 0, &active, &done);
            paths.len()
        };
        assert_eq!(cache.critical_builds(), 1);
        assert!(paths_len > 0);
    }

    #[test]
    fn cache_reuses_critical_paths_when_graph_unchanged() {
        let active = queue_file(vec![
            task("RQ-0001", vec![], TaskStatus::Todo),
            task("RQ-0002", vec!["RQ-0001"], TaskStatus::Todo),
        ]);
        let done = empty_done();
        let mut cache = DependencyGraphCache::new();

        // First access
        let _ = cache.critical_paths(0, 0, &active, &done);
        assert_eq!(cache.critical_builds(), 1);

        // Second access
        let _ = cache.critical_paths(0, 0, &active, &done);
        assert_eq!(cache.critical_builds(), 1); // No recomputation
    }

    #[test]
    fn cache_invalidates_critical_paths_on_graph_rebuild() {
        let active = queue_file(vec![
            task("RQ-0001", vec![], TaskStatus::Todo),
            task("RQ-0002", vec!["RQ-0001"], TaskStatus::Todo),
        ]);
        let done = empty_done();
        let mut cache = DependencyGraphCache::new();

        // First access
        let _ = cache.critical_paths(0, 0, &active, &done);
        assert_eq!(cache.critical_builds(), 1);

        // Change revisions (rebuilds graph, invalidates critical paths)
        let _ = cache.critical_paths(1, 0, &active, &done);
        assert_eq!(cache.critical_builds(), 2); // Recomputed
    }

    #[test]
    fn cache_is_valid_returns_correct_value() {
        let active = queue_file(vec![task("RQ-0001", vec![], TaskStatus::Todo)]);
        let done = empty_done();
        let mut cache = DependencyGraphCache::new();

        // Initially invalid
        assert!(!cache.is_valid(0, 0));

        // After building
        let _ = cache.graph(0, 0, &active, &done);
        assert!(cache.is_valid(0, 0));

        // Different revisions
        assert!(!cache.is_valid(1, 0));
        assert!(!cache.is_valid(0, 1));
    }

    #[test]
    fn cache_invalidate_clears_cache() {
        let active = queue_file(vec![task("RQ-0001", vec![], TaskStatus::Todo)]);
        let done = empty_done();
        let mut cache = DependencyGraphCache::new();

        // Build cache
        let _ = cache.graph(0, 0, &active, &done);
        assert!(cache.is_valid(0, 0));

        // Invalidate
        cache.invalidate();
        assert!(!cache.is_valid(0, 0));
        assert!(!cache.is_valid(u64::MAX, u64::MAX));
    }

    #[test]
    fn cache_handles_empty_queues() {
        let active = empty_done();
        let done = empty_done();
        let mut cache = DependencyGraphCache::new();

        let graph = cache.graph(0, 0, &active, &done);
        assert!(graph.is_empty());

        let paths = cache.critical_paths(0, 0, &active, &done);
        assert!(paths.is_empty());
    }

    #[test]
    fn cache_includes_done_queue_in_graph() {
        let active = queue_file(vec![task("RQ-0002", vec!["RQ-0001"], TaskStatus::Todo)]);
        let done = queue_file(vec![task("RQ-0001", vec![], TaskStatus::Done)]);
        let mut cache = DependencyGraphCache::new();

        let graph = cache.graph(0, 0, &active, &done);

        assert_eq!(graph.len(), 2);
        assert!(graph.contains("RQ-0001"));
        assert!(graph.contains("RQ-0002"));
    }
}
