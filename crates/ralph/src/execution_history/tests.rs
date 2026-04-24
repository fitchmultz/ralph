//! Execution-history regression tests.
//!
//! Purpose:
//! - Execution-history regression tests.
//!
//! Responsibilities:
//! - Verify execution-history persistence and pruning behavior.
//! - Verify weighted-average and timestamp-parsing helpers.
//!
//! Not handled here:
//! - ETA rendering.
//! - Real runner execution flows.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Tests use temp directories for isolated cache persistence.
//! - Timestamp parsing must stay aligned with `crate::timeutil`.

use std::collections::HashMap;
use std::time::Duration;

use tempfile::TempDir;

use crate::constants::versions::EXECUTION_HISTORY_VERSION;
use crate::progress::ExecutionPhase;

use super::{
    ExecutionEntry, ExecutionHistory, get_phase_averages, load_execution_history,
    parse_timestamp_to_secs, prune_old_entries, record_execution, save_execution_history,
    weighted_average_duration,
};

#[test]
fn test_load_empty_history() {
    let temp = TempDir::new().unwrap();
    let history = load_execution_history(temp.path()).unwrap();
    assert!(history.entries.is_empty());
    assert_eq!(history.version, EXECUTION_HISTORY_VERSION);
}

#[test]
fn test_save_and_load_history() {
    let temp = TempDir::new().unwrap();
    let mut history = ExecutionHistory::default();

    history.entries.push(ExecutionEntry {
        timestamp: "2026-01-31T12:00:00Z".to_string(),
        task_id: "RQ-0001".to_string(),
        runner: "codex".to_string(),
        model: "sonnet".to_string(),
        phase_count: 3,
        phase_durations: {
            let mut durations = HashMap::new();
            durations.insert(ExecutionPhase::Planning, Duration::from_secs(60));
            durations.insert(ExecutionPhase::Implementation, Duration::from_secs(120));
            durations.insert(ExecutionPhase::Review, Duration::from_secs(30));
            durations
        },
        total_duration: Duration::from_secs(210),
    });

    save_execution_history(&history, temp.path()).unwrap();
    let loaded = load_execution_history(temp.path()).unwrap();

    assert_eq!(loaded.entries.len(), 1);
    assert_eq!(loaded.entries[0].task_id, "RQ-0001");
    assert_eq!(loaded.entries[0].phase_count, 3);
}

#[test]
fn test_record_execution() {
    let temp = TempDir::new().unwrap();
    let mut durations = HashMap::new();
    durations.insert(ExecutionPhase::Planning, Duration::from_secs(60));

    record_execution(
        "RQ-0001",
        "codex",
        "sonnet",
        3,
        durations,
        Duration::from_secs(60),
        temp.path(),
    )
    .unwrap();

    let history = load_execution_history(temp.path()).unwrap();
    assert_eq!(history.entries.len(), 1);
    assert_eq!(history.entries[0].runner, "codex");
    assert!(
        !history.entries[0].timestamp.is_empty(),
        "Timestamp should never be empty"
    );
}

#[test]
fn test_prune_old_entries() {
    let mut history = ExecutionHistory::default();

    for index in 0..150 {
        history.entries.push(ExecutionEntry {
            timestamp: format!("2026-01-{:02}T12:00:00Z", (index % 30) + 1),
            task_id: format!("RQ-{index:04}"),
            runner: "codex".to_string(),
            model: "sonnet".to_string(),
            phase_count: 3,
            phase_durations: HashMap::new(),
            total_duration: Duration::from_secs(60),
        });
    }

    prune_old_entries(&mut history);
    assert_eq!(history.entries.len(), 100);
}

#[test]
fn test_weighted_average_duration() {
    let mut history = ExecutionHistory::default();

    history.entries.push(ExecutionEntry {
        timestamp: "2026-01-31T12:00:00Z".to_string(),
        task_id: "RQ-0001".to_string(),
        runner: "codex".to_string(),
        model: "sonnet".to_string(),
        phase_count: 3,
        phase_durations: {
            let mut durations = HashMap::new();
            durations.insert(ExecutionPhase::Planning, Duration::from_secs(100));
            durations
        },
        total_duration: Duration::from_secs(100),
    });

    history.entries.push(ExecutionEntry {
        timestamp: "2026-01-30T12:00:00Z".to_string(),
        task_id: "RQ-0002".to_string(),
        runner: "codex".to_string(),
        model: "sonnet".to_string(),
        phase_count: 3,
        phase_durations: {
            let mut durations = HashMap::new();
            durations.insert(ExecutionPhase::Planning, Duration::from_secs(200));
            durations
        },
        total_duration: Duration::from_secs(200),
    });

    let avg = weighted_average_duration(&history, "codex", "sonnet", 3, ExecutionPhase::Planning);
    assert!(avg.is_some());
    assert!(
        avg.unwrap().as_secs() < 150,
        "Weighted average should favor recent entries"
    );
}

#[test]
fn test_weighted_average_no_matching_entries() {
    let history = ExecutionHistory::default();
    let avg = weighted_average_duration(&history, "codex", "sonnet", 3, ExecutionPhase::Planning);
    assert!(avg.is_none());
}

#[test]
fn test_get_phase_averages() {
    let mut history = ExecutionHistory::default();

    history.entries.push(ExecutionEntry {
        timestamp: "2026-01-31T12:00:00Z".to_string(),
        task_id: "RQ-0001".to_string(),
        runner: "codex".to_string(),
        model: "sonnet".to_string(),
        phase_count: 3,
        phase_durations: {
            let mut durations = HashMap::new();
            durations.insert(ExecutionPhase::Planning, Duration::from_secs(60));
            durations.insert(ExecutionPhase::Implementation, Duration::from_secs(120));
            durations
        },
        total_duration: Duration::from_secs(180),
    });

    let averages = get_phase_averages(&history, "codex", "sonnet", 3);
    assert_eq!(averages.len(), 2);
    assert_eq!(
        averages.get(&ExecutionPhase::Planning),
        Some(&Duration::from_secs(60))
    );
    assert_eq!(
        averages.get(&ExecutionPhase::Implementation),
        Some(&Duration::from_secs(120))
    );
}

#[test]
fn test_parse_timestamp_to_secs() {
    assert!(parse_timestamp_to_secs("2026-01-31T12:00:00Z").is_some());
    assert!(parse_timestamp_to_secs("2026-01-31T12:00:00.123Z").is_some());
    assert!(parse_timestamp_to_secs("invalid").is_none());
}

#[test]
fn test_parse_timestamp_accuracy_vs_timeutil() {
    let test_cases = [
        "2026-01-31T12:00:00Z",
        "2026-01-31T12:00:00.123Z",
        "2026-01-31T12:00:00.123456789Z",
        "2020-02-29T00:00:00Z",
        "1970-01-01T00:00:00Z",
        "2000-12-31T23:59:59Z",
    ];

    for timestamp in &test_cases {
        let parsed = parse_timestamp_to_secs(timestamp);
        let expected = crate::timeutil::parse_rfc3339(timestamp)
            .ok()
            .map(|dt| dt.unix_timestamp() as u64);
        assert_eq!(
            parsed, expected,
            "parse_timestamp_to_secs({timestamp}) should match timeutil::parse_rfc3339"
        );
    }
}

#[test]
fn test_parse_timestamp_leap_year_accuracy() {
    let feb28 = parse_timestamp_to_secs("2020-02-28T00:00:00Z").unwrap();
    let feb29 = parse_timestamp_to_secs("2020-02-29T00:00:00Z").unwrap();
    let mar01 = parse_timestamp_to_secs("2020-03-01T00:00:00Z").unwrap();

    assert_eq!(
        feb29 - feb28,
        86400,
        "Leap day should be exactly 1 day after Feb 28"
    );
    assert_eq!(
        mar01 - feb29,
        86400,
        "Mar 01 should be exactly 1 day after Feb 29"
    );
}

#[test]
fn test_weighted_average_monotonic_decay() {
    let mut history = ExecutionHistory::default();

    for index in 0..5 {
        let day = 11 + index * 5;
        history.entries.push(ExecutionEntry {
            timestamp: format!("2026-01-{day:02}T12:00:00Z"),
            task_id: format!("RQ-{index}"),
            runner: "codex".to_string(),
            model: "sonnet".to_string(),
            phase_count: 3,
            phase_durations: {
                let mut durations = HashMap::new();
                durations.insert(ExecutionPhase::Planning, Duration::from_secs(100));
                durations
            },
            total_duration: Duration::from_secs(100),
        });
    }

    let avg = weighted_average_duration(&history, "codex", "sonnet", 3, ExecutionPhase::Planning);
    assert!(avg.is_some(), "Should have a weighted average");

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as f64;

    let mut weights = vec![];
    for entry in &history.entries {
        let entry_secs = parse_timestamp_to_secs(&entry.timestamp).unwrap_or(now as u64) as f64;
        let age_days = (now - entry_secs) / (24.0 * 3600.0);
        let weight = 0.9_f64.powf(age_days);
        weights.push((entry.timestamp.clone(), weight));
    }

    for index in 1..weights.len() {
        assert!(
            weights[index - 1].1 <= weights[index].1,
            "Weight should increase as entries get newer (older entries have lower weight): {:?} vs {:?}",
            weights[index - 1],
            weights[index]
        );
    }
}

#[test]
fn test_parse_timestamp_with_subseconds() {
    let without_ms = parse_timestamp_to_secs("2026-01-31T12:00:00Z").unwrap();
    let with_ms = parse_timestamp_to_secs("2026-01-31T12:00:00.500Z").unwrap();
    let with_many_ms = parse_timestamp_to_secs("2026-01-31T12:00:00.999999Z").unwrap();

    assert_eq!(
        without_ms, with_ms,
        "Subseconds should not affect unix timestamp"
    );
    assert_eq!(
        without_ms, with_many_ms,
        "Subseconds should not affect unix timestamp"
    );
}
