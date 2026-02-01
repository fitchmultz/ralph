//! Progress indicators and spinners for long-running operations.
//!
//! Responsibilities:
//! - Define a unified `ProgressIndicator` trait for both TUI and CLI modes.
//! - Provide animated spinner state management for indeterminate progress.
//! - Track current operation descriptions and phase transitions.
//! - Support disabling progress indicators for CI/scripting environments.
//!
//! Not handled here:
//! - Actual rendering to terminal (delegated to TUI ratatui or CLI indicatif).
//! - Phase timing logic (handled by app_execution module).
//! - Log output streaming (handled by runner module).
//!
//! Invariants/assumptions:
//! - Progress indicators are created per-execution and dropped when done.
//! - Spinner animation frames update at a fixed interval (80ms).
//! - NO_COLOR environment variable is respected for colored output.

use crate::constants::spinners::DEFAULT_SPINNER_FRAMES;
use crate::constants::timeouts::SPINNER_UPDATE_INTERVAL_MS;
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};

/// Trait for progress indication across TUI and CLI modes.
///
/// Implementations handle the actual rendering while this trait
/// defines the common interface for phase and operation updates.
pub trait ProgressIndicator: Send + Sync {
    /// Set the current execution phase.
    fn set_phase(&mut self, phase: ExecutionPhase);

    /// Set the current operation description (e.g., "Running CI gate...").
    fn set_operation(&mut self, operation: &str);

    /// Update the spinner animation (called periodically).
    fn tick(&mut self);

    /// Mark the progress as complete.
    fn finish(&mut self);

    /// Check if progress indicators are enabled.
    fn is_enabled(&self) -> bool;
}

/// Execution phases for multi-phase task workflows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionPhase {
    /// Phase 1: Planning and analysis
    Planning,
    /// Phase 2: Implementation and CI
    Implementation,
    /// Phase 3: Review and completion
    Review,
    /// Execution completed
    Complete,
}

impl ExecutionPhase {
    /// Returns the human-readable name for this phase.
    pub fn as_str(&self) -> &'static str {
        match self {
            ExecutionPhase::Planning => "Planning",
            ExecutionPhase::Implementation => "Implementation",
            ExecutionPhase::Review => "Review",
            ExecutionPhase::Complete => "Complete",
        }
    }

    /// Returns the phase number (1-3) or 0 for Complete.
    pub fn phase_number(&self) -> u8 {
        match self {
            ExecutionPhase::Planning => 1,
            ExecutionPhase::Implementation => 2,
            ExecutionPhase::Review => 3,
            ExecutionPhase::Complete => 0,
        }
    }

    /// Returns an emoji/icon representation of the phase.
    pub fn icon(&self) -> &'static str {
        match self {
            ExecutionPhase::Planning => "▶",
            ExecutionPhase::Implementation => "⚙",
            ExecutionPhase::Review => "👁",
            ExecutionPhase::Complete => "✓",
        }
    }
}

/// Spinner state for animation.
#[derive(Debug)]
pub struct SpinnerState {
    /// Animation frames (e.g., braille patterns).
    frames: Vec<&'static str>,
    /// Current frame index.
    current_frame: usize,
    /// Last time the frame was updated.
    last_update: Instant,
    /// Update interval.
    update_interval: Duration,
}

impl Default for SpinnerState {
    fn default() -> Self {
        Self::new(DEFAULT_SPINNER_FRAMES)
    }
}

impl SpinnerState {
    /// Create a new spinner with the given frames.
    pub fn new(frames: &[&'static str]) -> Self {
        Self {
            frames: frames.to_vec(),
            current_frame: 0,
            last_update: Instant::now(),
            update_interval: Duration::from_millis(SPINNER_UPDATE_INTERVAL_MS),
        }
    }

    /// Get the current frame without advancing.
    pub fn current_frame(&self) -> &str {
        self.frames.get(self.current_frame).copied().unwrap_or("⠋")
    }

    /// Advance to the next frame if enough time has passed.
    /// Returns true if the frame was advanced.
    pub fn tick(&mut self) -> bool {
        let now = Instant::now();
        if now.duration_since(self.last_update) >= self.update_interval {
            self.current_frame = (self.current_frame + 1) % self.frames.len().max(1);
            self.last_update = now;
            true
        } else {
            false
        }
    }

    /// Force a frame advance regardless of timing.
    pub fn force_tick(&mut self) {
        self.current_frame = (self.current_frame + 1) % self.frames.len().max(1);
        self.last_update = Instant::now();
    }

    /// Reset the spinner to the first frame.
    pub fn reset(&mut self) {
        self.current_frame = 0;
        self.last_update = Instant::now();
    }
}

/// No-op progress indicator for disabled mode.
pub struct NoOpProgressIndicator;

impl ProgressIndicator for NoOpProgressIndicator {
    fn set_phase(&mut self, _phase: ExecutionPhase) {}

    fn set_operation(&mut self, _operation: &str) {}

    fn tick(&mut self) {}

    fn finish(&mut self) {}

    fn is_enabled(&self) -> bool {
        false
    }
}

/// CLI progress indicator using indicatif.
pub struct CliProgressIndicator {
    /// Multi-progress bar for managing multiple bars.
    #[allow(dead_code)]
    multi_progress: indicatif::MultiProgress,
    /// Main progress bar for phase indication.
    phase_bar: indicatif::ProgressBar,
    /// Current phase.
    current_phase: ExecutionPhase,
    /// Current operation description.
    operation: String,
    /// Spinner state for animation (kept for API consistency).
    #[allow(dead_code)]
    spinner: SpinnerState,
    /// Whether colors are enabled.
    color_enabled: bool,
    /// Current completion percentage (0-100).
    completion_percentage: u8,
    /// Whether to show the progress bar.
    show_progress_bar: bool,
}

impl CliProgressIndicator {
    /// Create a new CLI progress indicator.
    pub fn new(color_enabled: bool) -> Self {
        Self::with_progress_bar(color_enabled, false)
    }

    /// Create a new CLI progress indicator with optional progress bar.
    pub fn with_progress_bar(color_enabled: bool, show_progress_bar: bool) -> Self {
        let multi_progress = indicatif::MultiProgress::new();

        // Create the main phase bar with a spinner style
        let phase_bar = multi_progress.add(indicatif::ProgressBar::new_spinner());
        phase_bar.enable_steady_tick(Duration::from_millis(SPINNER_UPDATE_INTERVAL_MS));

        // Set initial style
        Self::update_style(
            &phase_bar,
            ExecutionPhase::Planning,
            "Starting...",
            color_enabled,
            0,
            show_progress_bar,
        );

        Self {
            multi_progress,
            phase_bar,
            current_phase: ExecutionPhase::Planning,
            operation: "Starting...".to_string(),
            spinner: SpinnerState::default(),
            color_enabled,
            completion_percentage: 0,
            show_progress_bar,
        }
    }

    /// Update the progress bar style based on current state.
    fn update_style(
        bar: &indicatif::ProgressBar,
        phase: ExecutionPhase,
        operation: &str,
        color_enabled: bool,
        completion_percentage: u8,
        show_progress_bar: bool,
    ) {
        let phase_name = phase.as_str();
        let icon = phase.icon();

        let style = if show_progress_bar {
            // Style with progress bar and percentage
            if color_enabled {
                indicatif::ProgressStyle::default_spinner()
                    .template("{spinner:.green} {prefix:.bold.cyan} {msg:.white} [{bar:20.cyan/blue}] {pos}% ({elapsed})")
                    .unwrap()
                    .tick_strings(DEFAULT_SPINNER_FRAMES)
                    .progress_chars("█░")
            } else {
                indicatif::ProgressStyle::default_spinner()
                    .template("{spinner} {prefix} {msg} [{bar:20}] {pos}% ({elapsed})")
                    .unwrap()
                    .tick_strings(DEFAULT_SPINNER_FRAMES)
                    .progress_chars("#-")
            }
        } else {
            // Original spinner-only style
            if color_enabled {
                indicatif::ProgressStyle::default_spinner()
                    .template("{spinner:.green} {prefix:.bold.cyan} {msg:.white} ({elapsed})")
                    .unwrap()
                    .tick_strings(DEFAULT_SPINNER_FRAMES)
            } else {
                indicatif::ProgressStyle::default_spinner()
                    .template("{spinner} {prefix} {msg} ({elapsed})")
                    .unwrap()
                    .tick_strings(DEFAULT_SPINNER_FRAMES)
            }
        };

        bar.set_style(style);
        bar.set_prefix(format!("{} {}", icon, phase_name));
        bar.set_message(operation.to_string());
        if show_progress_bar {
            bar.set_position(completion_percentage as u64);
        }
    }

    /// Update the completion percentage display.
    pub fn set_completion(&mut self, percentage: u8) {
        self.completion_percentage = percentage.min(100);
        if self.show_progress_bar {
            self.phase_bar
                .set_position(self.completion_percentage as u64);
        }
    }

    /// Enable or disable the progress bar display.
    pub fn set_show_progress_bar(&mut self, show: bool) {
        self.show_progress_bar = show;
        // Update style to reflect the change
        Self::update_style(
            &self.phase_bar,
            self.current_phase,
            &self.operation,
            self.color_enabled,
            self.completion_percentage,
            self.show_progress_bar,
        );
    }
}

impl ProgressIndicator for CliProgressIndicator {
    fn set_phase(&mut self, phase: ExecutionPhase) {
        self.current_phase = phase;
        Self::update_style(
            &self.phase_bar,
            phase,
            &self.operation,
            self.color_enabled,
            self.completion_percentage,
            self.show_progress_bar,
        );
    }

    fn set_operation(&mut self, operation: &str) {
        self.operation = operation.to_string();
        self.phase_bar.set_message(self.operation.clone());
    }

    fn tick(&mut self) {
        // The progress bar has steady tick enabled, so this is a no-op
        // but we keep it for the trait interface.
    }

    fn finish(&mut self) {
        self.phase_bar.finish_with_message("Complete");
    }

    fn is_enabled(&self) -> bool {
        true
    }
}

/// TUI progress indicator for use with ratatui.
///
/// This is a lightweight struct that holds state; actual rendering
/// is handled by the TUI rendering code in `tui/render/panels.rs`.
#[derive(Debug)]
pub struct TuiProgressIndicator {
    /// Current execution phase.
    current_phase: ExecutionPhase,
    /// Current operation description.
    operation: String,
    /// Spinner state for animation.
    spinner: SpinnerState,
    /// Whether the indicator is enabled.
    enabled: bool,
    /// Phase start times for elapsed tracking.
    phase_start_times: std::collections::HashMap<ExecutionPhase, Instant>,
    /// Completed phase durations.
    phase_completion_times: std::collections::HashMap<ExecutionPhase, Duration>,
}

impl Default for TuiProgressIndicator {
    fn default() -> Self {
        Self::new(true)
    }
}

impl TuiProgressIndicator {
    /// Create a new TUI progress indicator.
    pub fn new(enabled: bool) -> Self {
        let mut phase_start_times = std::collections::HashMap::new();
        phase_start_times.insert(ExecutionPhase::Planning, Instant::now());

        Self {
            current_phase: ExecutionPhase::Planning,
            operation: "Starting...".to_string(),
            spinner: SpinnerState::default(),
            enabled,
            phase_start_times,
            phase_completion_times: std::collections::HashMap::new(),
        }
    }

    /// Get the current spinner frame.
    pub fn spinner_frame(&self) -> &str {
        self.spinner.current_frame()
    }

    /// Get the current operation description.
    pub fn operation(&self) -> &str {
        &self.operation
    }

    /// Get the current phase.
    pub fn current_phase(&self) -> ExecutionPhase {
        self.current_phase
    }

    /// Get elapsed time for a specific phase.
    pub fn phase_elapsed(&self, phase: ExecutionPhase) -> Duration {
        if let Some(completed) = self.phase_completion_times.get(&phase) {
            *completed
        } else if let Some(start) = self.phase_start_times.get(&phase) {
            start.elapsed()
        } else {
            Duration::ZERO
        }
    }

    /// Check if a phase is completed.
    pub fn is_phase_completed(&self, phase: ExecutionPhase) -> bool {
        self.phase_completion_times.contains_key(&phase)
            || phase.phase_number() < self.current_phase.phase_number()
    }

    /// Check if a phase is currently active.
    pub fn is_phase_active(&self, phase: ExecutionPhase) -> bool {
        self.current_phase == phase
    }

    /// Format a duration as MM:SS.
    pub fn format_duration(duration: Duration) -> String {
        let total_secs = duration.as_secs();
        let mins = total_secs / 60;
        let secs = total_secs % 60;
        format!("{:02}:{:02}", mins, secs)
    }
}

impl ProgressIndicator for TuiProgressIndicator {
    fn set_phase(&mut self, phase: ExecutionPhase) {
        // Record completion time for current phase
        if let Some(start) = self.phase_start_times.get(&self.current_phase) {
            let elapsed = start.elapsed();
            self.phase_completion_times
                .insert(self.current_phase, elapsed);
        }

        // Start new phase
        self.current_phase = phase;
        if phase != ExecutionPhase::Complete {
            self.phase_start_times.insert(phase, Instant::now());
        }

        // Reset spinner for visual feedback
        self.spinner.reset();
    }

    fn set_operation(&mut self, operation: &str) {
        self.operation = operation.to_string();
    }

    fn tick(&mut self) {
        self.spinner.tick();
    }

    fn finish(&mut self) {
        self.set_phase(ExecutionPhase::Complete);
        self.operation = "Complete".to_string();
    }

    fn is_enabled(&self) -> bool {
        self.enabled
    }
}

/// Factory function to create the appropriate progress indicator.
///
/// # Arguments
/// * `enabled` - Whether progress indicators should be enabled at all.
/// * `is_tui` - Whether this is for TUI mode (true) or CLI mode (false).
/// * `color_enabled` - Whether colors should be used.
pub fn create_progress_indicator(
    enabled: bool,
    is_tui: bool,
    color_enabled: bool,
) -> Box<dyn ProgressIndicator> {
    if !enabled {
        return Box::new(NoOpProgressIndicator);
    }

    if is_tui {
        Box::new(TuiProgressIndicator::new(color_enabled))
    } else {
        Box::new(CliProgressIndicator::new(color_enabled))
    }
}

/// Check if progress indicators should be enabled based on environment.
///
/// Respects:
/// - Explicit `--no-progress` flag
/// - TTY detection (auto-disable if not a TTY)
/// - NO_COLOR environment variable (disables colors but keeps animation)
pub fn should_enable_progress(no_progress_flag: bool, is_tty: bool) -> bool {
    if no_progress_flag {
        return false;
    }

    // Auto-disable if not a TTY (unless explicitly requested)
    if !is_tty {
        return false;
    }

    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_execution_phase_as_str() {
        assert_eq!(ExecutionPhase::Planning.as_str(), "Planning");
        assert_eq!(ExecutionPhase::Implementation.as_str(), "Implementation");
        assert_eq!(ExecutionPhase::Review.as_str(), "Review");
        assert_eq!(ExecutionPhase::Complete.as_str(), "Complete");
    }

    #[test]
    fn test_execution_phase_number() {
        assert_eq!(ExecutionPhase::Planning.phase_number(), 1);
        assert_eq!(ExecutionPhase::Implementation.phase_number(), 2);
        assert_eq!(ExecutionPhase::Review.phase_number(), 3);
        assert_eq!(ExecutionPhase::Complete.phase_number(), 0);
    }

    #[test]
    fn test_spinner_state_advances() {
        let mut spinner = SpinnerState::default();
        let initial_frame = spinner.current_frame().to_string();

        // Force tick should advance
        spinner.force_tick();
        assert_ne!(spinner.current_frame(), initial_frame);
    }

    #[test]
    fn test_spinner_state_resets() {
        let mut spinner = SpinnerState::default();
        spinner.force_tick();
        spinner.force_tick();
        spinner.reset();
        assert_eq!(spinner.current_frame(), DEFAULT_SPINNER_FRAMES[0]);
    }

    #[test]
    fn test_noop_indicator() {
        let mut indicator = NoOpProgressIndicator;
        assert!(!indicator.is_enabled());
        indicator.set_phase(ExecutionPhase::Implementation);
        indicator.set_operation("Test");
        indicator.tick();
        indicator.finish();
        // Should not panic
    }

    #[test]
    fn test_tui_indicator_phase_transition() {
        let mut indicator = TuiProgressIndicator::new(true);
        assert_eq!(indicator.current_phase(), ExecutionPhase::Planning);

        indicator.set_phase(ExecutionPhase::Implementation);
        assert_eq!(indicator.current_phase(), ExecutionPhase::Implementation);
        assert!(indicator.is_phase_completed(ExecutionPhase::Planning));
        assert!(!indicator.is_phase_completed(ExecutionPhase::Implementation));
    }

    #[test]
    fn test_format_duration() {
        let duration = Duration::from_secs(125); // 2:05
        assert_eq!(TuiProgressIndicator::format_duration(duration), "02:05");

        let duration = Duration::from_secs(59); // 0:59
        assert_eq!(TuiProgressIndicator::format_duration(duration), "00:59");
    }

    #[test]
    fn test_should_enable_progress() {
        assert!(!should_enable_progress(true, true)); // --no-progress flag
        assert!(!should_enable_progress(false, false)); // not a TTY
        assert!(should_enable_progress(false, true)); // normal case
    }
}
