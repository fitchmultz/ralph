//! Tests for execution phase tracking, timing, and transitions.
//!
//! Responsibilities:
//! - Validate phase tracking initialization, transitions, and timing.
//! - Test phase completion detection and duration formatting.
//! - Exercise log line processing for phase detection.
//!
//! Not handled here:
//! - App state initialization, filtering, or palette matching (see other modules).

use super::super::app::*;
use super::QueueFile;
use crate::progress::ExecutionPhase;
use std::time::Duration;

#[test]
fn phase_tracking_reset_initializes_defaults() {
    let mut app = App::new(QueueFile::default());

    app.reset_phase_tracking(3);

    assert_eq!(app.execution_phase, ExecutionPhase::Planning);
    assert_eq!(app.configured_phases, 3);
    assert!(app.show_progress_panel);
    assert!(app.total_execution_start.is_some());
    assert!(app
        .phase_start_times
        .contains_key(&ExecutionPhase::Planning));
}

#[test]
fn phase_tracking_reset_clamps_phase_count() {
    let mut app = App::new(QueueFile::default());

    // Test clamping to max 3
    app.reset_phase_tracking(5);
    assert_eq!(app.configured_phases, 3);

    // Test clamping to min 1
    app.reset_phase_tracking(0);
    assert_eq!(app.configured_phases, 1);
}

#[test]
fn phase_transition_records_completion_time() {
    let mut app = App::new(QueueFile::default());
    app.reset_phase_tracking(3);

    // Transition to Implementation
    app.transition_to_phase(ExecutionPhase::Implementation);

    assert_eq!(app.execution_phase, ExecutionPhase::Implementation);
    assert!(app
        .phase_completion_times
        .contains_key(&ExecutionPhase::Planning));
    assert!(app
        .phase_start_times
        .contains_key(&ExecutionPhase::Implementation));
}

#[test]
fn phase_transition_to_complete_does_not_start_timer() {
    let mut app = App::new(QueueFile::default());
    app.reset_phase_tracking(3);

    app.transition_to_phase(ExecutionPhase::Complete);

    assert_eq!(app.execution_phase, ExecutionPhase::Complete);
    assert!(!app
        .phase_start_times
        .contains_key(&ExecutionPhase::Complete));
}

#[test]
fn phase_elapsed_returns_zero_for_unstarted_phase() {
    let app = App::new(QueueFile::default());

    let elapsed = app.phase_elapsed(ExecutionPhase::Review);

    assert_eq!(elapsed, Duration::ZERO);
}

#[test]
fn phase_elapsed_returns_duration_for_active_phase() {
    let mut app = App::new(QueueFile::default());
    app.reset_phase_tracking(3);

    let elapsed = app.phase_elapsed(ExecutionPhase::Planning);

    // Should be non-zero since we just started the phase
    assert!(elapsed > Duration::ZERO);
}

#[test]
fn phase_elapsed_returns_completed_duration() {
    let mut app = App::new(QueueFile::default());
    app.reset_phase_tracking(3);

    // Record a completion time manually
    app.phase_completion_times
        .insert(ExecutionPhase::Planning, Duration::from_secs(45));

    let elapsed = app.phase_elapsed(ExecutionPhase::Planning);

    assert_eq!(elapsed, Duration::from_secs(45));
}

#[test]
fn is_phase_completed_detects_completed_phases() {
    let mut app = App::new(QueueFile::default());
    app.reset_phase_tracking(3);

    // Initially, only Planning is active
    assert!(!app.is_phase_completed(ExecutionPhase::Planning));
    assert!(!app.is_phase_completed(ExecutionPhase::Implementation));

    // Transition to Implementation
    app.transition_to_phase(ExecutionPhase::Implementation);

    assert!(app.is_phase_completed(ExecutionPhase::Planning));
    assert!(!app.is_phase_completed(ExecutionPhase::Implementation));
}

#[test]
fn is_phase_active_detects_current_phase() {
    let mut app = App::new(QueueFile::default());
    app.reset_phase_tracking(3);

    assert!(app.is_phase_active(ExecutionPhase::Planning));
    assert!(!app.is_phase_active(ExecutionPhase::Implementation));

    app.transition_to_phase(ExecutionPhase::Implementation);

    assert!(!app.is_phase_active(ExecutionPhase::Planning));
    assert!(app.is_phase_active(ExecutionPhase::Implementation));
}

#[test]
fn format_duration_formats_mm_ss() {
    assert_eq!(App::format_duration(Duration::from_secs(0)), "00:00");
    assert_eq!(App::format_duration(Duration::from_secs(45)), "00:45");
    assert_eq!(App::format_duration(Duration::from_secs(60)), "01:00");
    assert_eq!(App::format_duration(Duration::from_secs(90)), "01:30");
    assert_eq!(App::format_duration(Duration::from_secs(3661)), "61:01");
}

#[test]
fn total_execution_time_returns_zero_when_not_started() {
    let app = App::new(QueueFile::default());

    assert_eq!(app.total_execution_time(), Duration::ZERO);
}

#[test]
fn total_execution_time_returns_elapsed_since_start() {
    let mut app = App::new(QueueFile::default());
    app.reset_phase_tracking(3);

    let elapsed = app.total_execution_time();

    assert!(elapsed > Duration::ZERO);
}

#[test]
fn process_log_line_detects_planning_mode() {
    let mut app = App::new(QueueFile::default());
    app.reset_phase_tracking(3);

    app.process_log_line_for_phase("Starting task # PLANNING MODE");

    assert!(app.is_phase_active(ExecutionPhase::Planning));
}

#[test]
fn process_log_line_detects_implementation_mode() {
    let mut app = App::new(QueueFile::default());
    app.reset_phase_tracking(3);

    app.process_log_line_for_phase("Switching to # IMPLEMENTATION MODE");

    assert!(app.is_phase_active(ExecutionPhase::Implementation));
}

#[test]
fn process_log_line_detects_review_mode() {
    let mut app = App::new(QueueFile::default());
    app.reset_phase_tracking(3);

    app.process_log_line_for_phase("Entering # CODE REVIEW MODE");

    assert!(app.is_phase_active(ExecutionPhase::Review));
}

#[test]
fn execution_phase_as_str_returns_human_readable() {
    assert_eq!(ExecutionPhase::Planning.as_str(), "Planning");
    assert_eq!(ExecutionPhase::Implementation.as_str(), "Implementation");
    assert_eq!(ExecutionPhase::Review.as_str(), "Review");
    assert_eq!(ExecutionPhase::Complete.as_str(), "Complete");
}

#[test]
fn execution_phase_number_returns_correct_values() {
    assert_eq!(ExecutionPhase::Planning.phase_number(), 1);
    assert_eq!(ExecutionPhase::Implementation.phase_number(), 2);
    assert_eq!(ExecutionPhase::Review.phase_number(), 3);
    assert_eq!(ExecutionPhase::Complete.phase_number(), 0);
}

// Completion percentage tests

#[test]
fn completion_percentage_zero_at_start() {
    let mut app = App::new(QueueFile::default());
    app.reset_phase_tracking(3);

    assert_eq!(app.completion_percentage(), 0);
}

#[test]
fn completion_percentage_after_first_phase() {
    let mut app = App::new(QueueFile::default());
    app.reset_phase_tracking(3);

    app.transition_to_phase(ExecutionPhase::Implementation);

    // 1/3 complete = 33%
    assert_eq!(app.completion_percentage(), 33);
}

#[test]
fn completion_percentage_after_second_phase() {
    let mut app = App::new(QueueFile::default());
    app.reset_phase_tracking(3);

    app.transition_to_phase(ExecutionPhase::Implementation);
    app.transition_to_phase(ExecutionPhase::Review);

    // 2/3 complete = 66.67%, truncated to 66
    assert_eq!(app.completion_percentage(), 66);
}

#[test]
fn completion_percentage_at_completion() {
    let mut app = App::new(QueueFile::default());
    app.reset_phase_tracking(3);

    app.transition_to_phase(ExecutionPhase::Implementation);
    app.transition_to_phase(ExecutionPhase::Review);
    app.transition_to_phase(ExecutionPhase::Complete);

    // 3/3 complete = 100%
    assert_eq!(app.completion_percentage(), 100);
}

#[test]
fn completion_percentage_single_phase() {
    let mut app = App::new(QueueFile::default());
    app.reset_phase_tracking(1);

    assert_eq!(app.completion_percentage(), 0);

    app.transition_to_phase(ExecutionPhase::Complete);

    // 1/1 complete = 100%
    assert_eq!(app.completion_percentage(), 100);
}

#[test]
fn completion_percentage_two_phases() {
    let mut app = App::new(QueueFile::default());
    app.reset_phase_tracking(2);

    assert_eq!(app.completion_percentage(), 0);

    app.transition_to_phase(ExecutionPhase::Implementation);

    // 1/2 complete = 50%
    assert_eq!(app.completion_percentage(), 50);

    app.transition_to_phase(ExecutionPhase::Complete);

    // 2/2 complete = 100%
    assert_eq!(app.completion_percentage(), 100);
}

#[test]
fn completion_percentage_clamps_to_100() {
    let mut app = App::new(QueueFile::default());
    app.reset_phase_tracking(3);

    // Complete all phases
    app.transition_to_phase(ExecutionPhase::Implementation);
    app.transition_to_phase(ExecutionPhase::Review);
    app.transition_to_phase(ExecutionPhase::Complete);

    // Should be exactly 100, not more
    assert_eq!(app.completion_percentage(), 100);
}

#[test]
fn phase_elapsed_map_returns_all_phases() {
    let mut app = App::new(QueueFile::default());
    app.reset_phase_tracking(3);

    // Transition through phases
    app.transition_to_phase(ExecutionPhase::Implementation);
    app.transition_to_phase(ExecutionPhase::Review);

    let elapsed_map = app.phase_elapsed_map();

    // Should have entries for all three phases
    assert!(elapsed_map.contains_key(&ExecutionPhase::Planning));
    assert!(elapsed_map.contains_key(&ExecutionPhase::Implementation));
    assert!(elapsed_map.contains_key(&ExecutionPhase::Review));

    // Planning and Implementation should have completed durations
    assert!(elapsed_map[&ExecutionPhase::Planning] > Duration::ZERO);
    assert!(elapsed_map[&ExecutionPhase::Implementation] > Duration::ZERO);
}
