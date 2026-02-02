//! Historical execution duration tracking for ETA estimation.
//!
//! Responsibilities:
//! - Record phase durations for completed task executions.
//! - Provide weighted average calculations for ETA estimation.
//! - Persist data to `.ralph/cache/execution_history.json`.
//!
//! Not handled here:
//! - Real-time progress tracking (see `app_execution.rs`).
//! - Actual rendering of progress indicators.
//!
//! Invariants/assumptions:
//! - Historical data is keyed by (runner, model, phase_count) for accuracy.
//! - Recent runs are weighted higher (exponential decay).
//! - Maximum 100 entries per key to prevent unbounded growth.

use crate::constants::versions::EXECUTION_HISTORY_VERSION;
use crate::progress::ExecutionPhase;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::Path;
use std::time::Duration;

/// Root execution history data structure.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionHistory {
    /// Schema version for migrations.
    pub version: u32,
    /// Historical execution entries.
    pub entries: Vec<ExecutionEntry>,
}

impl Default for ExecutionHistory {
    fn default() -> Self {
        Self {
            version: EXECUTION_HISTORY_VERSION,
            entries: Vec::new(),
        }
    }
}

/// A single execution entry recording phase durations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionEntry {
    /// When the execution completed (RFC3339).
    pub timestamp: String,
    /// Task ID that was executed.
    pub task_id: String,
    /// Runner used (e.g., "codex", "claude").
    pub runner: String,
    /// Model used (e.g., "sonnet", "gpt-4").
    pub model: String,
    /// Number of phases configured (1, 2, or 3).
    pub phase_count: u8,
    /// Duration for each completed phase.
    pub phase_durations: HashMap<ExecutionPhase, Duration>,
    /// Total execution duration.
    pub total_duration: Duration,
}

/// Load execution history from cache directory.
pub fn load_execution_history(cache_dir: &Path) -> Result<ExecutionHistory> {
    let path = cache_dir.join("execution_history.json");

    if !path.exists() {
        return Ok(ExecutionHistory::default());
    }

    let content = fs::read_to_string(&path)
        .with_context(|| format!("Failed to read execution history from {}", path.display()))?;

    let history: ExecutionHistory = serde_json::from_str(&content)
        .with_context(|| format!("Failed to parse execution history from {}", path.display()))?;

    Ok(history)
}

/// Save execution history to cache directory.
pub fn save_execution_history(history: &ExecutionHistory, cache_dir: &Path) -> Result<()> {
    let path = cache_dir.join("execution_history.json");

    // Ensure cache directory exists
    fs::create_dir_all(cache_dir)
        .with_context(|| format!("Failed to create cache directory {}", cache_dir.display()))?;

    let content =
        serde_json::to_string_pretty(history).context("Failed to serialize execution history")?;

    // Atomic write: write to temp file then rename
    let temp_path = path.with_extension("tmp");
    let mut file = fs::File::create(&temp_path)
        .with_context(|| format!("Failed to create temp file {}", temp_path.display()))?;
    file.write_all(content.as_bytes())
        .with_context(|| format!("Failed to write to temp file {}", temp_path.display()))?;
    file.flush()
        .with_context(|| format!("Failed to flush temp file {}", temp_path.display()))?;
    drop(file);

    fs::rename(&temp_path, &path)
        .with_context(|| format!("Failed to rename temp file to {}", path.display()))?;

    Ok(())
}

/// Record a completed execution to history.
pub fn record_execution(
    task_id: &str,
    runner: &str,
    model: &str,
    phase_count: u8,
    phase_durations: HashMap<ExecutionPhase, Duration>,
    total_duration: Duration,
    cache_dir: &Path,
) -> Result<()> {
    let mut history = load_execution_history(cache_dir)?;

    let entry = ExecutionEntry {
        timestamp: crate::timeutil::now_utc_rfc3339().unwrap_or_default(),
        task_id: task_id.to_string(),
        runner: runner.to_string(),
        model: model.to_string(),
        phase_count,
        phase_durations,
        total_duration,
    };

    history.entries.push(entry);

    // Prune old entries if we exceed the limit
    prune_old_entries(&mut history);

    save_execution_history(&history, cache_dir)?;
    Ok(())
}

/// Prune oldest entries to keep history bounded.
fn prune_old_entries(history: &mut ExecutionHistory) {
    const MAX_ENTRIES: usize = 100;

    if history.entries.len() <= MAX_ENTRIES {
        return;
    }

    // Sort by timestamp (newest first) and keep only MAX_ENTRIES
    history
        .entries
        .sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
    history.entries.truncate(MAX_ENTRIES);
}

/// Calculate weighted average duration for a specific phase.
///
/// Uses exponential weighting where recent entries are weighted higher.
/// weight = 0.9^(age_in_days)
pub fn weighted_average_duration(
    history: &ExecutionHistory,
    runner: &str,
    model: &str,
    phase_count: u8,
    phase: ExecutionPhase,
) -> Option<Duration> {
    let relevant_entries: Vec<_> = history
        .entries
        .iter()
        .filter(|e| {
            e.runner == runner
                && e.model == model
                && e.phase_count == phase_count
                && e.phase_durations.contains_key(&phase)
        })
        .collect();

    if relevant_entries.is_empty() {
        return None;
    }

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as f64;

    let mut total_weight = 0.0;
    let mut weighted_sum = 0.0;

    for entry in relevant_entries {
        let entry_secs = parse_timestamp_to_secs(&entry.timestamp).unwrap_or(now as u64) as f64;
        let age_days = (now - entry_secs) / (24.0 * 3600.0);
        let weight = 0.9_f64.powf(age_days);

        if let Some(duration) = entry.phase_durations.get(&phase) {
            weighted_sum += duration.as_secs_f64() * weight;
            total_weight += weight;
        }
    }

    if total_weight == 0.0 {
        return None;
    }

    let avg_secs = weighted_sum / total_weight;
    Some(Duration::from_secs_f64(avg_secs))
}

/// Get historical average durations for all phases.
pub fn get_phase_averages(
    history: &ExecutionHistory,
    runner: &str,
    model: &str,
    phase_count: u8,
) -> HashMap<ExecutionPhase, Duration> {
    let mut averages = HashMap::new();

    for phase in [
        ExecutionPhase::Planning,
        ExecutionPhase::Implementation,
        ExecutionPhase::Review,
    ] {
        if let Some(avg) = weighted_average_duration(history, runner, model, phase_count, phase) {
            averages.insert(phase, avg);
        }
    }

    averages
}

/// Parse RFC3339 timestamp to Unix seconds.
fn parse_timestamp_to_secs(timestamp: &str) -> Option<u64> {
    // Simple parsing for RFC3339 format: 2026-01-31T12:00:00Z or 2026-01-31T12:00:00.000Z
    let timestamp = timestamp.split('.').next()?;
    let parts: Vec<&str> = timestamp.split('T').collect();
    if parts.len() != 2 {
        return None;
    }

    let date_parts: Vec<&str> = parts[0].split('-').collect();
    let time_parts: Vec<&str> = parts[1].split(':').collect();

    if date_parts.len() != 3 || time_parts.len() < 2 {
        return None;
    }

    let year: i32 = date_parts[0].parse().ok()?;
    let month: u32 = date_parts[1].parse().ok()?;
    let day: u32 = date_parts[2].parse().ok()?;
    let hour: u32 = time_parts[0].parse().ok()?;
    let minute: u32 = time_parts[1].parse().ok()?;
    let second: u32 = time_parts
        .get(2)
        .and_then(|s| s.trim_end_matches('Z').parse().ok())
        .unwrap_or(0);

    // Simplified conversion (approximate)
    let days_since_epoch = (year - 1970) * 365 + month as i32 * 30 + day as i32;
    let secs = days_since_epoch as u64 * 24 * 3600
        + hour as u64 * 3600
        + minute as u64 * 60
        + second as u64;

    Some(secs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

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
                let mut d = HashMap::new();
                d.insert(ExecutionPhase::Planning, Duration::from_secs(60));
                d.insert(ExecutionPhase::Implementation, Duration::from_secs(120));
                d.insert(ExecutionPhase::Review, Duration::from_secs(30));
                d
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
    }

    #[test]
    fn test_prune_old_entries() {
        let mut history = ExecutionHistory::default();

        // Add 150 entries
        for i in 0..150 {
            history.entries.push(ExecutionEntry {
                timestamp: format!("2026-01-{:02}T12:00:00Z", (i % 30) + 1),
                task_id: format!("RQ-{:04}", i),
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

        // Add entries with different timestamps
        history.entries.push(ExecutionEntry {
            timestamp: "2026-01-31T12:00:00Z".to_string(), // Recent
            task_id: "RQ-0001".to_string(),
            runner: "codex".to_string(),
            model: "sonnet".to_string(),
            phase_count: 3,
            phase_durations: {
                let mut d = HashMap::new();
                d.insert(ExecutionPhase::Planning, Duration::from_secs(100));
                d
            },
            total_duration: Duration::from_secs(100),
        });

        history.entries.push(ExecutionEntry {
            timestamp: "2026-01-30T12:00:00Z".to_string(), // Older
            task_id: "RQ-0002".to_string(),
            runner: "codex".to_string(),
            model: "sonnet".to_string(),
            phase_count: 3,
            phase_durations: {
                let mut d = HashMap::new();
                d.insert(ExecutionPhase::Planning, Duration::from_secs(200));
                d
            },
            total_duration: Duration::from_secs(200),
        });

        let avg =
            weighted_average_duration(&history, "codex", "sonnet", 3, ExecutionPhase::Planning);
        assert!(avg.is_some());
        // Recent entry (100s) should be weighted higher than older (200s)
        let avg_secs = avg.unwrap().as_secs();
        assert!(
            avg_secs < 150,
            "Weighted average should favor recent entries"
        );
    }

    #[test]
    fn test_weighted_average_no_matching_entries() {
        let history = ExecutionHistory::default();
        let avg =
            weighted_average_duration(&history, "codex", "sonnet", 3, ExecutionPhase::Planning);
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
                let mut d = HashMap::new();
                d.insert(ExecutionPhase::Planning, Duration::from_secs(60));
                d.insert(ExecutionPhase::Implementation, Duration::from_secs(120));
                d
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
        let secs = parse_timestamp_to_secs("2026-01-31T12:00:00Z");
        assert!(secs.is_some());

        let secs_with_ms = parse_timestamp_to_secs("2026-01-31T12:00:00.123Z");
        assert!(secs_with_ms.is_some());

        let invalid = parse_timestamp_to_secs("invalid");
        assert!(invalid.is_none());
    }
}
