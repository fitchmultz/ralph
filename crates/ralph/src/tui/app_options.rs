//! TUI boot options and configuration types.
//!
//! Responsibilities:
//! - Define options that control how the TUI boots (loop mode, flowchart, etc.).
//! - Provide types for filter cache statistics (test-only).
//!
//! Not handled here:
//! - Runtime TUI state management (see app module).
//! - Filter state management (see app_filters module).
//! - Terminal capability detection (see terminal module).
//!
//! Invariants/assumptions:
//! - TuiOptions is immutable after creation and passed to the TUI at startup.
//! - FilterCacheStats is only used in test builds for cache hit/miss tracking.

use super::terminal::{BorderStyle, ColorOption, ColorSupport, TerminalCapabilities};

/// Options that control how the TUI boots.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TuiOptions {
    /// If true, start loop mode immediately after launch.
    pub start_loop: bool,
    /// Optional max tasks for loop mode (None = unlimited).
    pub loop_max_tasks: Option<u32>,
    /// If true, draft tasks are eligible for loop selection.
    pub loop_include_draft: bool,
    /// If true, show flowchart visualization on start.
    pub show_flowchart: bool,
    /// If true, disable mouse capture.
    pub no_mouse: bool,
    /// Color output control.
    pub color: ColorOption,
    /// If true, use ASCII borders instead of Unicode.
    pub ascii_borders: bool,
    /// If true, disable progress indicators and spinners.
    pub no_progress: bool,
}

impl TuiOptions {
    /// Create a new TuiOptions with all defaults.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set start_loop option.
    pub fn with_start_loop(mut self, start_loop: bool) -> Self {
        self.start_loop = start_loop;
        self
    }

    /// Set loop_max_tasks option.
    pub fn with_loop_max_tasks(mut self, max_tasks: Option<u32>) -> Self {
        self.loop_max_tasks = max_tasks;
        self
    }

    /// Set loop_include_draft option.
    pub fn with_loop_include_draft(mut self, include_draft: bool) -> Self {
        self.loop_include_draft = include_draft;
        self
    }

    /// Set show_flowchart option.
    pub fn with_show_flowchart(mut self, show_flowchart: bool) -> Self {
        self.show_flowchart = show_flowchart;
        self
    }

    /// Set no_mouse option.
    pub fn with_no_mouse(mut self, no_mouse: bool) -> Self {
        self.no_mouse = no_mouse;
        self
    }

    /// Set color option.
    pub fn with_color(mut self, color: ColorOption) -> Self {
        self.color = color;
        self
    }

    /// Set ascii_borders option.
    pub fn with_ascii_borders(mut self, ascii_borders: bool) -> Self {
        self.ascii_borders = ascii_borders;
        self
    }

    /// Set no_progress option.
    pub fn with_no_progress(mut self, no_progress: bool) -> Self {
        self.no_progress = no_progress;
        self
    }

    /// Resolve the effective color support based on options and terminal capabilities.
    pub fn resolve_color_support(&self, capabilities: TerminalCapabilities) -> ColorSupport {
        self.color.resolve(capabilities.colors)
    }

    /// Determine the border style based on options and terminal capabilities.
    pub fn resolve_border_style(&self, capabilities: TerminalCapabilities) -> BorderStyle {
        BorderStyle::for_capabilities(capabilities, self.ascii_borders)
    }

    /// Determine if mouse should be enabled based on options and terminal capabilities.
    pub fn should_enable_mouse(&self, capabilities: TerminalCapabilities) -> bool {
        !self.no_mouse && capabilities.has_mouse()
    }
}

/// Statistics for filter cache performance tracking.
///
/// This struct is only available in test builds and is used to verify
/// that cache invalidation and rebuilding works correctly.
#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FilterCacheStats {
    /// Number of times the ID-to-index map was rebuilt.
    pub id_index_rebuilds: usize,
    /// Number of times the filtered indices were rebuilt.
    pub filtered_rebuilds: usize,
}

#[cfg(test)]
impl FilterCacheStats {
    /// Create new cache stats with zero values.
    pub fn new() -> Self {
        Self {
            id_index_rebuilds: 0,
            filtered_rebuilds: 0,
        }
    }
}

#[cfg(test)]
impl Default for FilterCacheStats {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tui_options_default() {
        let opts = TuiOptions::default();
        assert!(!opts.start_loop);
        assert_eq!(opts.loop_max_tasks, None);
        assert!(!opts.loop_include_draft);
        assert!(!opts.show_flowchart);
        assert!(!opts.no_mouse);
        assert!(!opts.ascii_borders);
        assert!(!opts.no_progress);
    }

    #[test]
    fn test_tui_options_builder() {
        let opts = TuiOptions::new()
            .with_start_loop(true)
            .with_loop_max_tasks(Some(5))
            .with_loop_include_draft(true)
            .with_show_flowchart(true)
            .with_no_mouse(true)
            .with_ascii_borders(true)
            .with_no_progress(true);

        assert!(opts.start_loop);
        assert_eq!(opts.loop_max_tasks, Some(5));
        assert!(opts.loop_include_draft);
        assert!(opts.show_flowchart);
        assert!(opts.no_mouse);
        assert!(opts.ascii_borders);
        assert!(opts.no_progress);
    }

    #[test]
    fn test_filter_cache_stats() {
        let stats = FilterCacheStats::new();
        assert_eq!(stats.id_index_rebuilds, 0);
        assert_eq!(stats.filtered_rebuilds, 0);
    }
}
