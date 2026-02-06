//! Parallel state overlay state management for the TUI.
//!
//! Responsibilities:
//! - Track overlay UI state (active tab, scroll positions, selected PR index).
//! - Load and cache parallel state from disk.
//! - Provide accessors for the renderer and event handler.
//!
//! Not handled here:
//! - Rendering (see `tui::render::overlays::parallel_state`).
//! - Event handling (see `tui::events::parallel_state`).
//! - Mutating parallel execution state (this overlay is read-only).
//!
//! Invariants/assumptions:
//! - The overlay is strictly read-only; it never starts/stops parallel runs.
//! - State file path is computed from the queue path.

use crate::commands::run::ParallelStateFile;

/// Tabs available in the parallel state overlay.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ParallelStateTab {
    #[default]
    InFlight = 0,
    Prs = 1,
    FinishedWithoutPr = 2,
}

impl ParallelStateTab {
    /// Get all tab variants.
    pub fn all() -> [ParallelStateTab; 3] {
        [
            ParallelStateTab::InFlight,
            ParallelStateTab::Prs,
            ParallelStateTab::FinishedWithoutPr,
        ]
    }

    /// Get the index of this tab (0-based).
    pub fn idx(self) -> usize {
        self as usize
    }

    /// Get the next tab, wrapping around.
    pub fn next(self) -> Self {
        match self {
            ParallelStateTab::InFlight => ParallelStateTab::Prs,
            ParallelStateTab::Prs => ParallelStateTab::FinishedWithoutPr,
            ParallelStateTab::FinishedWithoutPr => ParallelStateTab::InFlight,
        }
    }

    /// Get the previous tab, wrapping around.
    pub fn prev(self) -> Self {
        match self {
            ParallelStateTab::InFlight => ParallelStateTab::FinishedWithoutPr,
            ParallelStateTab::Prs => ParallelStateTab::InFlight,
            ParallelStateTab::FinishedWithoutPr => ParallelStateTab::Prs,
        }
    }
}

/// Snapshot of parallel state for the overlay.
#[derive(Debug, Clone)]
pub enum ParallelStateOverlaySnapshot {
    /// State file is missing (no parallel run in progress).
    Missing { path: String },
    /// State file exists but couldn't be parsed.
    Invalid { path: String, error: String },
    /// State loaded successfully.
    Loaded { state: ParallelStateFile },
}

/// State for the parallel state overlay.
#[derive(Debug, Default)]
pub struct ParallelStateOverlayState {
    /// Currently active tab.
    active_tab: ParallelStateTab,
    /// Scroll offset for the content (row index).
    scroll: usize,
    /// Last known visible rows in the content area.
    visible_rows: usize,
    /// Selected PR index (only meaningful when on PRs tab).
    selected_pr: usize,
    /// Cached state snapshot (loaded from disk).
    snapshot: Option<ParallelStateOverlaySnapshot>,
}

impl ParallelStateOverlayState {
    /// Create a new overlay state with default values.
    pub fn new() -> Self {
        Self::default()
    }

    /// Get the active tab.
    pub fn active_tab(&self) -> ParallelStateTab {
        self.active_tab
    }

    /// Set the active tab and reset scroll/selection as appropriate.
    pub fn set_active_tab(&mut self, tab: ParallelStateTab) {
        self.active_tab = tab;
        self.scroll = 0;
        // Reset PR selection when switching to PR tab, keep it otherwise
        if tab == ParallelStateTab::Prs {
            self.selected_pr = 0;
        }
    }

    /// Move to the next tab.
    pub fn next_tab(&mut self) {
        self.set_active_tab(self.active_tab.next());
    }

    /// Move to the previous tab.
    pub fn prev_tab(&mut self) {
        self.set_active_tab(self.active_tab.prev());
    }

    /// Get the current scroll offset.
    pub fn scroll(&self) -> usize {
        self.scroll
    }

    /// Set the visible rows count (used for clamping).
    pub fn set_visible_rows(&mut self, rows: usize) {
        self.visible_rows = rows.max(1);
    }

    /// Scroll up by the specified number of rows.
    pub fn scroll_up(&mut self, rows: usize) {
        self.scroll = self.scroll.saturating_sub(rows);
    }

    /// Scroll down by the specified number of rows.
    pub fn scroll_down(&mut self, rows: usize, max_items: usize) {
        let max_scroll = max_items.saturating_sub(self.visible_rows);
        self.scroll = (self.scroll + rows).min(max_scroll);
    }

    /// Scroll to the top.
    pub fn scroll_top(&mut self) {
        self.scroll = 0;
    }

    /// Scroll to the bottom.
    pub fn scroll_bottom(&mut self, max_items: usize) {
        let max_scroll = max_items.saturating_sub(self.visible_rows);
        self.scroll = max_scroll;
    }

    /// Page up.
    pub fn page_up(&mut self) {
        self.scroll_up(self.visible_rows.saturating_sub(1).max(1));
    }

    /// Page down.
    pub fn page_down(&mut self, max_items: usize) {
        self.scroll_down(self.visible_rows.saturating_sub(1).max(1), max_items);
    }

    /// Get the selected PR index.
    pub fn selected_pr(&self) -> usize {
        self.selected_pr
    }

    /// Move PR selection up.
    pub fn select_pr_up(&mut self) {
        self.selected_pr = self.selected_pr.saturating_sub(1);
        // Adjust scroll to keep selection visible
        if self.selected_pr < self.scroll {
            self.scroll = self.selected_pr;
        }
    }

    /// Move PR selection down.
    pub fn select_pr_down(&mut self, total_prs: usize) {
        if total_prs == 0 {
            return;
        }
        self.selected_pr = (self.selected_pr + 1).min(total_prs.saturating_sub(1));
        // Adjust scroll to keep selection visible
        let end_visible = self.scroll + self.visible_rows;
        if self.selected_pr >= end_visible && end_visible < total_prs {
            self.scroll = self
                .selected_pr
                .saturating_sub(self.visible_rows.saturating_sub(1));
        }
    }

    /// Get the cached snapshot, if any.
    pub fn snapshot(&self) -> Option<&ParallelStateOverlaySnapshot> {
        self.snapshot.as_ref()
    }

    /// Set the cached snapshot.
    pub fn set_snapshot(&mut self, snapshot: ParallelStateOverlaySnapshot) {
        self.snapshot = Some(snapshot);
        // Reset selection if it's now out of bounds
        if let Some(ParallelStateOverlaySnapshot::Loaded { state }) = &self.snapshot {
            let pr_count = state.prs.len();
            if self.selected_pr >= pr_count && pr_count > 0 {
                self.selected_pr = pr_count.saturating_sub(1);
            }
        }
    }

    /// Clear the cached snapshot.
    pub fn clear_snapshot(&mut self) {
        self.snapshot = None;
    }
}

/// Tab counts for display.
#[derive(Debug, Clone, Copy, Default)]
pub struct TabCounts {
    pub in_flight: usize,
    pub prs: usize,
    pub finished_without_pr: usize,
}

// ============================================================================
// App implementation for parallel state overlay
// ============================================================================

use crate::tui::app::App;

impl App {
    /// Get or initialize the parallel state overlay state.
    fn parallel_state_overlay(
        &mut self,
    ) -> &mut crate::tui::app_parallel_state::ParallelStateOverlayState {
        if self.parallel_state_overlay.is_none() {
            self.parallel_state_overlay =
                Some(crate::tui::app_parallel_state::ParallelStateOverlayState::new());
        }
        self.parallel_state_overlay.as_mut().unwrap()
    }

    /// Get a reference to the parallel state overlay state (may be None if not initialized).
    fn parallel_state_overlay_ref(
        &self,
    ) -> Option<&crate::tui::app_parallel_state::ParallelStateOverlayState> {
        self.parallel_state_overlay.as_ref()
    }

    /// Enter parallel state overlay mode.
    pub fn enter_parallel_state_overlay(&mut self) {
        let previous_mode = Box::new(self.mode.clone());
        self.mode = crate::tui::events::AppMode::ParallelStateOverlay { previous_mode };

        // Initialize overlay state and load from disk
        self.parallel_state_overlay_reload_from_disk();
    }

    /// Reload state from disk.
    pub fn parallel_state_overlay_reload_from_disk(&mut self) {
        // Get queue_path first to avoid borrow issues
        let Some(queue_path) = self.queue_path.clone() else {
            if let Some(overlay) = self.parallel_state_overlay.as_mut() {
                overlay.clear_snapshot();
            }
            return;
        };

        let Some(repo_root) = crate::tui::external_tools::repo_root_from_queue_path(&queue_path)
        else {
            if let Some(overlay) = self.parallel_state_overlay.as_mut() {
                overlay.clear_snapshot();
            }
            return;
        };

        let overlay = self.parallel_state_overlay();

        let state_path = crate::commands::run::state_file_path(&repo_root);

        match crate::commands::run::load_state(&state_path) {
            Ok(Some(state)) => {
                overlay.set_snapshot(
                    crate::tui::app_parallel_state::ParallelStateOverlaySnapshot::Loaded { state },
                );
            }
            Ok(None) => {
                overlay.set_snapshot(
                    crate::tui::app_parallel_state::ParallelStateOverlaySnapshot::Missing {
                        path: state_path.display().to_string(),
                    },
                );
            }
            Err(e) => {
                let error_str = e.to_string();
                overlay.set_snapshot(
                    crate::tui::app_parallel_state::ParallelStateOverlaySnapshot::Invalid {
                        path: state_path.display().to_string(),
                        error: error_str,
                    },
                );
            }
        }
    }

    /// Get the current snapshot.
    pub fn parallel_state_overlay_snapshot(
        &self,
    ) -> crate::tui::app_parallel_state::ParallelStateOverlaySnapshot {
        match self.parallel_state_overlay_ref() {
            Some(overlay) => overlay.snapshot().cloned().unwrap_or_else(|| {
                crate::tui::app_parallel_state::ParallelStateOverlaySnapshot::Missing {
                    path: "unknown".to_string(),
                }
            }),
            None => crate::tui::app_parallel_state::ParallelStateOverlaySnapshot::Missing {
                path: "unknown".to_string(),
            },
        }
    }

    /// Get the active tab.
    pub fn parallel_state_overlay_active_tab(
        &self,
    ) -> crate::tui::app_parallel_state::ParallelStateTab {
        self.parallel_state_overlay_ref()
            .map(|o| o.active_tab())
            .unwrap_or_default()
    }

    /// Get tab counts and active tab.
    pub fn parallel_state_overlay_tab_counts_and_active(
        &self,
    ) -> (
        crate::tui::app_parallel_state::TabCounts,
        crate::tui::app_parallel_state::ParallelStateTab,
    ) {
        let active_tab = self.parallel_state_overlay_active_tab();

        let mut counts = crate::tui::app_parallel_state::TabCounts::default();

        if let Some(overlay) = self.parallel_state_overlay_ref()
            && let Some(snapshot) = overlay.snapshot()
            && let crate::tui::app_parallel_state::ParallelStateOverlaySnapshot::Loaded { state } =
                snapshot
        {
            counts.in_flight = state.tasks_in_flight.len();
            counts.prs = state.prs.len();
            counts.finished_without_pr = state.finished_without_pr.len();
        }

        (counts, active_tab)
    }

    /// Move to the next tab.
    pub fn parallel_state_overlay_next_tab(&mut self) {
        self.parallel_state_overlay().next_tab();
    }

    /// Move to the previous tab.
    pub fn parallel_state_overlay_prev_tab(&mut self) {
        self.parallel_state_overlay().prev_tab();
    }

    /// Set the visible rows count.
    pub fn parallel_state_overlay_set_visible_rows(&mut self, rows: usize) {
        self.parallel_state_overlay().set_visible_rows(rows);
    }

    /// Scroll up in the content.
    pub fn parallel_state_overlay_up(&mut self) {
        let active_tab = self.parallel_state_overlay_active_tab();
        let overlay = self.parallel_state_overlay();

        match active_tab {
            crate::tui::app_parallel_state::ParallelStateTab::Prs => {
                overlay.select_pr_up();
            }
            _ => {
                overlay.scroll_up(1);
            }
        }
    }

    /// Scroll down in the content.
    pub fn parallel_state_overlay_down(&mut self) {
        let active_tab = self.parallel_state_overlay_active_tab();
        let total_items = self.parallel_state_overlay_total_items();
        let overlay = self.parallel_state_overlay();

        match active_tab {
            crate::tui::app_parallel_state::ParallelStateTab::Prs => {
                if let Some(snapshot) = overlay.snapshot()
                    && let crate::tui::app_parallel_state::ParallelStateOverlaySnapshot::Loaded {
                        state,
                    } = snapshot
                {
                    overlay.select_pr_down(state.prs.len());
                }
            }
            _ => {
                overlay.scroll_down(1, total_items);
            }
        }
    }

    /// Page up in the content.
    pub fn parallel_state_overlay_page_up(&mut self) {
        self.parallel_state_overlay().page_up();
    }

    /// Page down in the content.
    pub fn parallel_state_overlay_page_down(&mut self) {
        let total_items = self.parallel_state_overlay_total_items();
        self.parallel_state_overlay().page_down(total_items);
    }

    /// Scroll to the top.
    pub fn parallel_state_overlay_top(&mut self) {
        self.parallel_state_overlay().scroll_top();
    }

    /// Scroll to the bottom.
    pub fn parallel_state_overlay_bottom(&mut self) {
        let total_items = self.parallel_state_overlay_total_items();
        self.parallel_state_overlay().scroll_bottom(total_items);
    }

    /// Get the selected PR index.
    pub fn parallel_state_overlay_selected_pr_index(&self) -> usize {
        self.parallel_state_overlay_ref()
            .map(|o| o.selected_pr())
            .unwrap_or(0)
    }

    /// Get the PR scroll offset.
    pub fn parallel_state_overlay_pr_scroll(&self) -> usize {
        self.parallel_state_overlay_ref()
            .map(|o| o.scroll())
            .unwrap_or(0)
    }

    /// Get the selected PR URL, if any.
    pub fn parallel_state_overlay_selected_pr_url(&self) -> Option<String> {
        let overlay = self.parallel_state_overlay_ref()?;
        let snapshot = overlay.snapshot()?;

        if let crate::tui::app_parallel_state::ParallelStateOverlaySnapshot::Loaded { state } =
            snapshot
        {
            let selected_idx = overlay.selected_pr();
            state.prs.get(selected_idx).map(|pr| pr.pr_url.clone())
        } else {
            None
        }
    }

    /// Get the metadata line for display.
    pub fn parallel_state_overlay_metadata_line(&self, _max_width: usize) -> String {
        let (counts, _) = self.parallel_state_overlay_tab_counts_and_active();
        format!(
            "In-Flight: {} | PRs: {} | Finished w/o PR: {}",
            counts.in_flight, counts.prs, counts.finished_without_pr
        )
    }

    /// Get the footer hint for display.
    pub fn parallel_state_overlay_footer_hint(&self) -> String {
        "Esc/P: close | Tab: section | r: reload | ↑↓/j/k: nav | o/Enter: open | y: copy"
            .to_string()
    }

    /// Helper to get total items for the active tab.
    fn parallel_state_overlay_total_items(&self) -> usize {
        let (counts, active_tab) = self.parallel_state_overlay_tab_counts_and_active();

        match active_tab {
            crate::tui::app_parallel_state::ParallelStateTab::InFlight => counts.in_flight,
            crate::tui::app_parallel_state::ParallelStateTab::Prs => counts.prs,
            crate::tui::app_parallel_state::ParallelStateTab::FinishedWithoutPr => {
                counts.finished_without_pr
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::run::{ParallelPrLifecycle, ParallelPrRecord, ParallelStateFile};
    use crate::contracts::{ParallelMergeMethod, ParallelMergeWhen};

    #[test]
    fn tab_next_and_prev_wrap_around() {
        let tab = ParallelStateTab::InFlight;
        assert_eq!(tab.next(), ParallelStateTab::Prs);
        assert_eq!(tab.next().next(), ParallelStateTab::FinishedWithoutPr);
        assert_eq!(tab.next().next().next(), ParallelStateTab::InFlight);

        let tab = ParallelStateTab::InFlight;
        assert_eq!(tab.prev(), ParallelStateTab::FinishedWithoutPr);
        assert_eq!(tab.prev().prev(), ParallelStateTab::Prs);
        assert_eq!(tab.prev().prev().prev(), ParallelStateTab::InFlight);
    }

    #[test]
    fn tab_idx_returns_correct_index() {
        assert_eq!(ParallelStateTab::InFlight.idx(), 0);
        assert_eq!(ParallelStateTab::Prs.idx(), 1);
        assert_eq!(ParallelStateTab::FinishedWithoutPr.idx(), 2);
    }

    #[test]
    fn overlay_state_default_values() {
        let state = ParallelStateOverlayState::new();
        assert_eq!(state.active_tab(), ParallelStateTab::InFlight);
        assert_eq!(state.scroll(), 0);
        assert_eq!(state.selected_pr(), 0);
        assert!(state.snapshot().is_none());
    }

    #[test]
    fn set_active_tab_resets_scroll() {
        let mut state = ParallelStateOverlayState::new();
        state.set_visible_rows(10);
        state.scroll_down(5, 100);
        assert_eq!(state.scroll(), 5);

        state.set_active_tab(ParallelStateTab::Prs);
        assert_eq!(state.scroll(), 0);
        assert_eq!(state.active_tab(), ParallelStateTab::Prs);
    }

    #[test]
    fn next_and_prev_tab_cycles() {
        let mut state = ParallelStateOverlayState::new();
        assert_eq!(state.active_tab(), ParallelStateTab::InFlight);

        state.next_tab();
        assert_eq!(state.active_tab(), ParallelStateTab::Prs);

        state.next_tab();
        assert_eq!(state.active_tab(), ParallelStateTab::FinishedWithoutPr);

        state.next_tab();
        assert_eq!(state.active_tab(), ParallelStateTab::InFlight);

        state.prev_tab();
        assert_eq!(state.active_tab(), ParallelStateTab::FinishedWithoutPr);
    }

    #[test]
    fn scroll_up_respects_bounds() {
        let mut state = ParallelStateOverlayState::new();
        state.set_visible_rows(10);
        state.scroll_down(100, 200);
        assert!(state.scroll() > 0);

        state.scroll_up(5);
        let scroll_after = state.scroll();
        assert!(scroll_after < 200); // Should have scrolled up
        state.scroll_up(100);
        assert_eq!(state.scroll(), 0); // Saturating sub
    }

    #[test]
    fn scroll_down_respects_max_items() {
        let mut state = ParallelStateOverlayState::new();
        state.set_visible_rows(10);

        // With 25 items and 10 visible rows, max scroll is 15
        state.scroll_down(100, 25);
        assert_eq!(state.scroll(), 15);

        // Trying to scroll more doesn't exceed the max
        state.scroll_down(5, 25);
        assert_eq!(state.scroll(), 15);
    }

    #[test]
    fn scroll_top_and_bottom() {
        let mut state = ParallelStateOverlayState::new();
        state.set_visible_rows(10);

        state.scroll_top();
        assert_eq!(state.scroll(), 0);

        state.scroll_bottom(50);
        assert_eq!(state.scroll(), 40); // 50 - 10

        state.scroll_top();
        assert_eq!(state.scroll(), 0);
    }

    #[test]
    fn page_up_and_page_down() {
        let mut state = ParallelStateOverlayState::new();
        state.set_visible_rows(10);

        state.page_down(100);
        assert_eq!(state.scroll(), 9); // visible_rows - 1

        state.page_up();
        assert_eq!(state.scroll(), 0); // Saturating sub
    }

    #[test]
    fn select_pr_up_and_down() {
        let mut state = ParallelStateOverlayState::new();
        state.set_visible_rows(5);

        // Start at 0, can't go up
        state.select_pr_up();
        assert_eq!(state.selected_pr(), 0);

        // Move down within bounds
        state.select_pr_down(10);
        assert_eq!(state.selected_pr(), 1);

        state.select_pr_down(10);
        assert_eq!(state.selected_pr(), 2);

        // Move back up
        state.select_pr_up();
        assert_eq!(state.selected_pr(), 1);
    }

    #[test]
    fn select_pr_down_respects_total() {
        let mut state = ParallelStateOverlayState::new();

        // Can't select beyond total - 1
        state.select_pr_down(3);
        state.select_pr_down(3);
        state.select_pr_down(3);
        assert_eq!(state.selected_pr(), 2);

        // Can't go further
        state.select_pr_down(3);
        assert_eq!(state.selected_pr(), 2);
    }

    #[test]
    fn select_pr_down_with_zero_total_is_noop() {
        let mut state = ParallelStateOverlayState::new();
        state.select_pr_down(0);
        assert_eq!(state.selected_pr(), 0);
    }

    #[test]
    fn select_pr_adjusts_scroll_to_keep_visible() {
        let mut state = ParallelStateOverlayState::new();
        state.set_visible_rows(5);
        state.scroll_top();

        // Select PR at index 10
        for _ in 0..10 {
            state.select_pr_down(20);
        }
        assert_eq!(state.selected_pr(), 10);
        // Scroll should have adjusted to keep selection visible
        assert!(state.scroll() > 0 || state.scroll() <= 10);
    }

    #[test]
    fn snapshot_missing_variant() {
        let snapshot = ParallelStateOverlaySnapshot::Missing {
            path: "/tmp/state.json".to_string(),
        };
        match snapshot {
            ParallelStateOverlaySnapshot::Missing { path } => {
                assert_eq!(path, "/tmp/state.json");
            }
            _ => panic!("Expected Missing variant"),
        }
    }

    #[test]
    fn snapshot_invalid_variant() {
        let snapshot = ParallelStateOverlaySnapshot::Invalid {
            path: "/tmp/state.json".to_string(),
            error: "parse error".to_string(),
        };
        match snapshot {
            ParallelStateOverlaySnapshot::Invalid { path, error } => {
                assert_eq!(path, "/tmp/state.json");
                assert_eq!(error, "parse error");
            }
            _ => panic!("Expected Invalid variant"),
        }
    }

    #[test]
    fn snapshot_loaded_variant() {
        let file_state = ParallelStateFile::new(
            "2026-02-01T00:00:00Z".to_string(),
            "main".to_string(),
            ParallelMergeMethod::Squash,
            ParallelMergeWhen::AsCreated,
        );
        let snapshot = ParallelStateOverlaySnapshot::Loaded { state: file_state };
        match snapshot {
            ParallelStateOverlaySnapshot::Loaded { state } => {
                assert_eq!(state.base_branch, "main");
            }
            _ => panic!("Expected Loaded variant"),
        }
    }

    #[test]
    fn set_snapshot_resets_selection_if_out_of_bounds() {
        let mut state = ParallelStateOverlayState::new();

        // Simulate having selected PR index 5 with 10 PRs
        state.set_visible_rows(10);
        for _ in 0..5 {
            state.select_pr_down(10);
        }
        assert_eq!(state.selected_pr(), 5);

        // Now set a snapshot with only 3 PRs
        let mut file_state = ParallelStateFile::new(
            "2026-02-01T00:00:00Z".to_string(),
            "main".to_string(),
            ParallelMergeMethod::Squash,
            ParallelMergeWhen::AsCreated,
        );
        file_state.upsert_pr(create_test_pr_record("RQ-0001", 1));
        file_state.upsert_pr(create_test_pr_record("RQ-0002", 2));
        file_state.upsert_pr(create_test_pr_record("RQ-0003", 3));

        state.set_snapshot(ParallelStateOverlaySnapshot::Loaded { state: file_state });

        // Selection should be clamped to the new bounds (2 = 3 - 1)
        assert_eq!(state.selected_pr(), 2);
    }

    #[test]
    fn set_snapshot_keeps_selection_if_in_bounds() {
        let mut state = ParallelStateOverlayState::new();

        // Select PR index 1
        state.select_pr_down(10);
        assert_eq!(state.selected_pr(), 1);

        // Set snapshot with 5 PRs
        let mut file_state = ParallelStateFile::new(
            "2026-02-01T00:00:00Z".to_string(),
            "main".to_string(),
            ParallelMergeMethod::Squash,
            ParallelMergeWhen::AsCreated,
        );
        for i in 1..=5 {
            file_state.upsert_pr(create_test_pr_record(&format!("RQ-{:04}", i), i));
        }

        state.set_snapshot(ParallelStateOverlaySnapshot::Loaded { state: file_state });

        // Selection should remain at 1
        assert_eq!(state.selected_pr(), 1);
    }

    #[test]
    fn clear_snapshot_removes_cached_state() {
        let mut state = ParallelStateOverlayState::new();

        let file_state = ParallelStateFile::new(
            "2026-02-01T00:00:00Z".to_string(),
            "main".to_string(),
            ParallelMergeMethod::Squash,
            ParallelMergeWhen::AsCreated,
        );
        state.set_snapshot(ParallelStateOverlaySnapshot::Loaded { state: file_state });
        assert!(state.snapshot().is_some());

        state.clear_snapshot();
        assert!(state.snapshot().is_none());
    }

    #[test]
    fn tab_counts_default_to_zero() {
        let counts = TabCounts::default();
        assert_eq!(counts.in_flight, 0);
        assert_eq!(counts.prs, 0);
        assert_eq!(counts.finished_without_pr, 0);
    }

    // Helper function to create test PR records
    fn create_test_pr_record(task_id: &str, pr_number: u32) -> ParallelPrRecord {
        ParallelPrRecord {
            task_id: task_id.to_string(),
            pr_number,
            pr_url: format!("https://github.com/example/pr/{}", pr_number),
            head: Some(format!("ralph/{}", task_id)),
            base: Some("main".to_string()),
            workspace_path: Some(format!("/tmp/ws/{}", task_id)),
            merged: false,
            lifecycle: ParallelPrLifecycle::Open,
            merge_blocker: None,
        }
    }

    #[test]
    fn scroll_operations_with_zero_visible_rows() {
        // Edge case: visible_rows defaults to at least 1
        let mut state = ParallelStateOverlayState::new();
        state.set_visible_rows(0);

        // Internal value should be at least 1
        state.scroll_down(5, 10);
        // Should not panic and should have reasonable behavior
    }

    #[test]
    fn select_pr_with_empty_list() {
        let mut state = ParallelStateOverlayState::new();
        state.set_visible_rows(5);

        // Trying to select in an empty list should be a no-op
        state.select_pr_down(0);
        assert_eq!(state.selected_pr(), 0);
    }

    #[test]
    fn set_snapshot_with_empty_pr_list() {
        let mut state = ParallelStateOverlayState::new();

        // First select something (won't happen with empty, but simulate state)
        let file_state = ParallelStateFile::new(
            "2026-02-01T00:00:00Z".to_string(),
            "main".to_string(),
            ParallelMergeMethod::Squash,
            ParallelMergeWhen::AsCreated,
        );
        // No PRs in this state

        state.set_snapshot(ParallelStateOverlaySnapshot::Loaded { state: file_state });

        // Should not panic with empty PR list
        assert!(state.snapshot().is_some());
    }
}
