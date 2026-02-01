//! Log management state for the TUI.
//!
//! Responsibilities:
//! - Store execution logs with a size limit (circular buffer behavior).
//! - Store raw ANSI bytes for terminal emulation display (tui-term integration).
//! - Track scroll position and autoscroll state.
//! - Provide methods for appending lines and scrolling.
//!
//! Not handled here:
//! - Log rendering (handled by render module).
//! - Log content generation (handled by runner and other modules).
//! - Phase detection from log lines (handled by execution state module).
//!
//! Invariants/assumptions:
//! - Maximum log size is 10,000 lines; older lines are dropped when exceeded.
//! - ANSI buffer grows with raw output; vt100 parser recreates screen state each render.
//! - Autoscroll automatically moves to the bottom when new lines are added.
//! - Manual scrolling disables autoscroll.

#![allow(dead_code)]

use crate::constants::buffers::{MAX_ANSI_BUFFER_SIZE, MAX_LOG_LINES};

/// State for execution log management.
#[derive(Debug)]
pub struct LogState {
    /// Execution logs (text lines for display and scrolling).
    pub logs: Vec<String>,
    /// Raw ANSI bytes for terminal emulation display (tui-term integration).
    pub ansi_buffer: Vec<u8>,
    /// Scroll offset for execution logs.
    pub scroll: usize,
    /// Whether to auto-scroll execution logs.
    pub autoscroll: bool,
    /// Last known visible log lines in Executing view (for paging/auto-scroll).
    visible_lines: usize,
}

impl Default for LogState {
    fn default() -> Self {
        Self {
            logs: Vec::new(),
            ansi_buffer: Vec::new(),
            scroll: 0,
            autoscroll: true,
            visible_lines: 20,
        }
    }
}

impl LogState {
    /// Create a new log state with default values.
    pub fn new() -> Self {
        Self::default()
    }

    /// Get the number of visible lines.
    pub fn visible_lines(&self) -> usize {
        self.visible_lines.max(1)
    }

    /// Update the number of visible lines and adjust scroll if needed.
    pub fn set_visible_lines(&mut self, visible_lines: usize) {
        let visible_lines = visible_lines.max(1);
        self.visible_lines = visible_lines;
        let max_scroll = self.max_scroll();
        if self.autoscroll || self.scroll > max_scroll {
            self.scroll = max_scroll;
        }
    }

    /// Calculate the maximum valid scroll position.
    pub fn max_scroll(&self) -> usize {
        self.logs.len().saturating_sub(self.visible_lines())
    }

    /// Append lines to the log buffer.
    ///
    /// If the buffer exceeds `MAX_LOG_LINES`, old lines are removed.
    /// If autoscroll is enabled, scrolls to the bottom.
    /// Also appends to the ANSI buffer for terminal emulation display.
    pub fn append_lines<I>(&mut self, lines: I)
    where
        I: IntoIterator<Item = String>,
    {
        for line in lines {
            // Add to text logs
            self.logs.push(line.clone());
            // Add to ANSI buffer with newline for terminal emulation
            self.ansi_buffer.extend_from_slice(line.as_bytes());
            self.ansi_buffer.push(b'\n');
        }

        // Trim old lines if we exceed the maximum
        if self.logs.len() > MAX_LOG_LINES {
            let excess = self.logs.len() - MAX_LOG_LINES;
            self.logs.drain(0..excess);
            self.scroll = self.scroll.saturating_sub(excess);
        }

        // Trim ANSI buffer if it exceeds the maximum size
        if self.ansi_buffer.len() > MAX_ANSI_BUFFER_SIZE {
            let excess = self.ansi_buffer.len() - MAX_ANSI_BUFFER_SIZE;
            self.ansi_buffer.drain(0..excess);
        }

        // Autoscroll if enabled
        if self.autoscroll {
            self.scroll = self.max_scroll();
        }
    }

    /// Clear all logs and reset scroll.
    pub fn clear(&mut self) {
        self.logs.clear();
        self.ansi_buffer.clear();
        self.scroll = 0;
    }

    /// Scroll up by the specified number of lines.
    /// Disables autoscroll.
    pub fn scroll_up(&mut self, lines: usize) {
        if lines == 0 {
            return;
        }
        self.autoscroll = false;
        self.scroll = self.scroll.saturating_sub(lines);
    }

    /// Scroll down by the specified number of lines.
    /// Disables autoscroll.
    pub fn scroll_down(&mut self, lines: usize) {
        if lines == 0 {
            return;
        }
        self.autoscroll = false;
        let max_scroll = self.max_scroll();
        self.scroll = (self.scroll + lines).min(max_scroll);
    }

    /// Enable autoscroll and scroll to the bottom.
    pub fn enable_autoscroll(&mut self) {
        self.autoscroll = true;
        self.scroll = self.max_scroll();
    }

    /// Disable autoscroll.
    pub fn disable_autoscroll(&mut self) {
        self.autoscroll = false;
    }

    /// Get the number of log lines.
    pub fn len(&self) -> usize {
        self.logs.len()
    }

    /// Check if there are no logs.
    pub fn is_empty(&self) -> bool {
        self.logs.is_empty()
    }

    /// Get a reference to the logs.
    pub fn logs(&self) -> &[String] {
        &self.logs
    }

    /// Append a formatted error message to the logs.
    ///
    /// Extracts the first non-empty line as a summary and adds
    /// the full error details to the log buffer.
    pub fn append_error(&mut self, summary: &str, details: &str) {
        let mut lines = Vec::new();
        lines.push(summary.to_string());

        if details.trim().is_empty() {
            lines.push("(no details provided)".to_string());
        } else {
            for line in details.lines() {
                lines.push(line.to_string());
            }
        }

        self.append_lines(lines);
    }

    /// Create a vt100 parser with the current ANSI buffer processed.
    ///
    /// Returns a parser configured with the given dimensions and the
    /// ANSI buffer processed through it. This is used by tui-term's
    /// PseudoTerminal widget for rendering ANSI-aware output.
    pub fn create_vt100_parser(&self, rows: u16, cols: u16) -> vt100::Parser {
        let mut parser = vt100::Parser::new(rows, cols, 0);
        parser.process(&self.ansi_buffer);
        parser
    }
}

// ============================================================================
// LogOperations trait for App
// ============================================================================

use crate::tui::App;

/// Trait for log scrolling operations.
pub trait LogOperations {
    /// Get the maximum log scroll position.
    fn max_log_scroll(&self, visible_lines: usize) -> usize;

    /// Scroll logs up by a number of lines.
    fn scroll_logs_up(&mut self, lines: usize);

    /// Scroll logs down by a number of lines.
    fn scroll_logs_down(&mut self, lines: usize, visible_lines: usize);

    /// Enable autoscroll.
    fn enable_autoscroll(&mut self, visible_lines: usize);
}

impl LogOperations for App {
    fn max_log_scroll(&self, visible_lines: usize) -> usize {
        self.logs.len().saturating_sub(visible_lines)
    }

    fn scroll_logs_up(&mut self, lines: usize) {
        self.autoscroll = false;
        self.log_scroll = self.log_scroll.saturating_sub(lines);
    }

    fn scroll_logs_down(&mut self, lines: usize, visible_lines: usize) {
        self.autoscroll = false;
        let max_scroll = self.max_log_scroll(visible_lines);
        self.log_scroll = (self.log_scroll + lines).min(max_scroll);
    }

    fn enable_autoscroll(&mut self, visible_lines: usize) {
        self.autoscroll = true;
        self.log_scroll = self.max_log_scroll(visible_lines);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_append_lines() {
        let mut state = LogState::new();
        state.append_lines(vec!["line1".to_string(), "line2".to_string()]);

        assert_eq!(state.len(), 2);
        assert_eq!(state.logs[0], "line1");
        assert_eq!(state.logs[1], "line2");
        // Verify ANSI buffer is also populated
        assert_eq!(state.ansi_buffer, b"line1\nline2\n");
    }

    #[test]
    fn test_autoscroll_on_append() {
        let mut state = LogState::new();
        state.set_visible_lines(5);

        // Add more lines than visible
        for i in 0..10 {
            state.append_lines(vec![format!("line{}", i)]);
        }

        // Should have scrolled to show the last 5 lines
        assert_eq!(state.scroll, 5);
        assert!(state.autoscroll);
    }

    #[test]
    fn test_manual_scroll_disables_autoscroll() {
        let mut state = LogState::new();
        state.set_visible_lines(5);

        for i in 0..10 {
            state.append_lines(vec![format!("line{}", i)]);
        }

        // Scroll up should disable autoscroll
        state.scroll_up(2);
        assert!(!state.autoscroll);
        assert_eq!(state.scroll, 3);
    }

    #[test]
    fn test_size_limit() {
        let mut state = LogState::new();

        // Add many lines to trigger trimming
        for i in 0..MAX_LOG_LINES + 100 {
            state.append_lines(vec![format!("line{}", i)]);
        }

        assert_eq!(state.len(), MAX_LOG_LINES);
        // Should contain the most recent lines
        assert!(state.logs[0].contains("100"));
    }

    #[test]
    fn test_clear() {
        let mut state = LogState::new();
        state.append_lines(vec!["line1".to_string()]);
        state.scroll_up(1);

        state.clear();

        assert!(state.is_empty());
        assert_eq!(state.scroll, 0);
        // Verify ANSI buffer is also cleared
        assert!(state.ansi_buffer.is_empty());
    }

    #[test]
    fn test_append_error() {
        let mut state = LogState::new();
        state.append_error("Error summary", "Detail 1\nDetail 2");

        assert_eq!(state.logs[0], "Error summary");
        assert_eq!(state.logs[1], "Detail 1");
        assert_eq!(state.logs[2], "Detail 2");
    }

    #[test]
    fn test_ansi_buffer_populated() {
        let mut state = LogState::new();
        state.append_lines(vec!["Hello".to_string(), "World".to_string()]);

        // ANSI buffer should contain lines with newlines
        assert_eq!(state.ansi_buffer, b"Hello\nWorld\n");
    }

    #[test]
    fn test_create_vt100_parser() {
        let mut state = LogState::new();
        // Add some ANSI-colored output
        state.append_lines(vec![
            "\x1b[32mGreen text\x1b[0m".to_string(),
            "Normal text".to_string(),
        ]);

        // Create parser and verify it processes the buffer
        let parser = state.create_vt100_parser(10, 80);
        let screen = parser.screen();

        // Screen should have the configured dimensions
        assert_eq!(screen.size(), (10, 80));
        // The screen should have processed the buffer without errors
        // (actual content format depends on vt100 crate internals)
    }

    #[test]
    fn test_ansi_buffer_size_limit() {
        let mut state = LogState::new();

        // Add a large line that would exceed the buffer limit if added many times
        let large_line = "x".repeat(1000);
        for _ in 0..(MAX_ANSI_BUFFER_SIZE / 1000 + 10) {
            state.append_lines(vec![large_line.clone()]);
        }

        // Buffer should be trimmed to max size
        assert!(state.ansi_buffer.len() <= MAX_ANSI_BUFFER_SIZE);
    }
}
