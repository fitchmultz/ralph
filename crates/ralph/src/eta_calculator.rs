//! ETA calculation for task execution based on historical data.
//!
//! Responsibilities:
//! - Calculate estimated time remaining using historical averages.
//! - Support weighted averages (recent runs weighted higher).
//! - Fall back to current-phase-only estimation when no history.
//!
//! Not handled here:
//! - Historical data persistence (see `execution_history.rs`).
//! - Rendering of ETA display.
//!
//! Invariants/assumptions:
//! - ETA estimates are based on historical phase durations for the same (runner, model, phase_count) combination.
//! - Confidence levels are determined by the amount of historical data available.

use crate::execution_history::{get_phase_averages, load_execution_history, ExecutionHistory};
use crate::progress::ExecutionPhase;
use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;

/// ETA estimate with confidence level.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EtaEstimate {
    /// Estimated time remaining.
    pub remaining: Duration,
    /// Confidence level based on historical data availability.
    pub confidence: EtaConfidence,
    /// Whether the estimate is based on historical data.
    pub based_on_history: bool,
}

/// Confidence level for ETA estimates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EtaConfidence {
    /// High confidence (>5 historical entries).
    High,
    /// Medium confidence (2-5 entries).
    Medium,
    /// Low confidence (<2 entries or fallback).
    Low,
}

impl EtaConfidence {
    /// Returns a visual indicator for the confidence level.
    pub fn indicator(&self) -> &'static str {
        match self {
            EtaConfidence::High => "●",
            EtaConfidence::Medium => "◐",
            EtaConfidence::Low => "○",
        }
    }

    /// Returns a color name for the confidence level (for TUI styling).
    pub fn color_name(&self) -> &'static str {
        match self {
            EtaConfidence::High => "green",
            EtaConfidence::Medium => "yellow",
            EtaConfidence::Low => "gray",
        }
    }
}

/// Calculator for ETA estimates based on historical execution data.
#[derive(Debug, Clone)]
pub struct EtaCalculator {
    history: ExecutionHistory,
}

impl EtaCalculator {
    /// Create a new ETA calculator with the given history.
    pub fn new(history: ExecutionHistory) -> Self {
        Self { history }
    }

    /// Create an empty calculator with no historical data.
    pub fn empty() -> Self {
        Self {
            history: ExecutionHistory::default(),
        }
    }

    /// Load the ETA calculator from cache directory.
    pub fn load(cache_dir: &Path) -> Self {
        match load_execution_history(cache_dir) {
            Ok(history) => Self::new(history),
            Err(_) => Self::empty(),
        }
    }

    /// Calculate ETA based on current progress and historical data.
    ///
    /// # Arguments
    /// * `runner` - The runner being used (e.g., "codex", "claude").
    /// * `model` - The model being used (e.g., "sonnet", "gpt-4").
    /// * `phase_count` - Number of phases configured (1, 2, or 3).
    /// * `current_phase` - The currently active phase.
    /// * `phase_elapsed` - Map of elapsed time for each phase.
    pub fn calculate_eta(
        &self,
        runner: &str,
        model: &str,
        phase_count: u8,
        current_phase: ExecutionPhase,
        phase_elapsed: &HashMap<ExecutionPhase, Duration>,
    ) -> Option<EtaEstimate> {
        if phase_count == 0 {
            return None;
        }

        // Get historical averages for all phases
        let averages = get_phase_averages(&self.history, runner, model, phase_count);

        // Count how many historical entries we have for confidence calculation
        let entry_count = self
            .history
            .entries
            .iter()
            .filter(|e| e.runner == runner && e.model == model && e.phase_count == phase_count)
            .count();

        let confidence = if entry_count >= 5 {
            EtaConfidence::High
        } else if entry_count >= 2 {
            EtaConfidence::Medium
        } else {
            EtaConfidence::Low
        };

        let based_on_history = !averages.is_empty();

        // Calculate remaining time
        let remaining = if based_on_history {
            self.calculate_with_history(phase_count, current_phase, phase_elapsed, &averages)
        } else {
            self.calculate_without_history(phase_count, current_phase, phase_elapsed)
        };

        Some(EtaEstimate {
            remaining,
            confidence,
            based_on_history,
        })
    }

    /// Calculate ETA using historical averages.
    fn calculate_with_history(
        &self,
        phase_count: u8,
        current_phase: ExecutionPhase,
        phase_elapsed: &HashMap<ExecutionPhase, Duration>,
        averages: &HashMap<ExecutionPhase, Duration>,
    ) -> Duration {
        let mut total_remaining = Duration::ZERO;

        // For each phase that hasn't completed yet
        let phases = match phase_count {
            1 => vec![ExecutionPhase::Planning],
            2 => vec![ExecutionPhase::Planning, ExecutionPhase::Implementation],
            _ => vec![
                ExecutionPhase::Planning,
                ExecutionPhase::Implementation,
                ExecutionPhase::Review,
            ],
        };

        for phase in phases {
            let elapsed = phase_elapsed.get(&phase).copied().unwrap_or(Duration::ZERO);

            if phase == current_phase {
                // Current phase: estimate remaining based on historical average
                if let Some(&avg) = averages.get(&phase) {
                    if elapsed < avg {
                        total_remaining += avg - elapsed;
                    }
                    // If we've exceeded average, add a small buffer based on current elapsed
                    else {
                        total_remaining += Duration::from_secs(elapsed.as_secs() / 10);
                    }
                } else {
                    // No history for this phase, use current elapsed as estimate
                    total_remaining += elapsed;
                }
            } else if !self.is_phase_completed(phase, current_phase) {
                // Future phase: use historical average
                if let Some(&avg) = averages.get(&phase) {
                    total_remaining += avg;
                } else {
                    // No history, use average of other phases as fallback
                    let fallback = self.calculate_fallback_average(averages);
                    total_remaining += fallback;
                }
            }
            // Completed phases contribute nothing to remaining time
        }

        total_remaining
    }

    /// Calculate ETA without historical data (simple heuristic).
    fn calculate_without_history(
        &self,
        phase_count: u8,
        current_phase: ExecutionPhase,
        phase_elapsed: &HashMap<ExecutionPhase, Duration>,
    ) -> Duration {
        let phases = match phase_count {
            1 => vec![ExecutionPhase::Planning],
            2 => vec![ExecutionPhase::Planning, ExecutionPhase::Implementation],
            _ => vec![
                ExecutionPhase::Planning,
                ExecutionPhase::Implementation,
                ExecutionPhase::Review,
            ],
        };

        let mut total_remaining = Duration::ZERO;
        let current_elapsed = phase_elapsed
            .get(&current_phase)
            .copied()
            .unwrap_or(Duration::ZERO);

        // Count remaining phases (including current)
        let remaining_phases = phases
            .iter()
            .filter(|&&p| !self.is_phase_completed(p, current_phase))
            .count();

        if remaining_phases > 0 {
            // Assume current phase will take about as long as elapsed so far
            // (i.e., 50% complete assumption)
            let current_remaining = current_elapsed;
            total_remaining += current_remaining;

            // For future phases, assume they take similar time to completed phases
            let completed_count = phase_count as usize - remaining_phases;
            if completed_count > 0 {
                let completed_total: Duration = phases
                    .iter()
                    .filter(|&&p| self.is_phase_completed(p, current_phase))
                    .filter_map(|&p| phase_elapsed.get(&p).copied())
                    .fold(Duration::ZERO, |acc, d| acc + d);
                let avg_completed = completed_total / completed_count as u32;

                // Add estimate for remaining future phases (excluding current)
                total_remaining += avg_completed * (remaining_phases.saturating_sub(1) as u32);
            } else {
                // No completed phases, use current elapsed as estimate for remaining phases
                total_remaining += current_elapsed * (remaining_phases.saturating_sub(1) as u32);
            }
        }

        total_remaining
    }

    /// Check if a phase is completed based on current phase.
    fn is_phase_completed(&self, phase: ExecutionPhase, current_phase: ExecutionPhase) -> bool {
        phase.phase_number() < current_phase.phase_number()
    }

    /// Calculate fallback average from available historical data.
    fn calculate_fallback_average(&self, averages: &HashMap<ExecutionPhase, Duration>) -> Duration {
        if averages.is_empty() {
            return Duration::from_secs(60); // Default 1 minute fallback
        }

        let total: Duration = averages
            .values()
            .copied()
            .fold(Duration::ZERO, |acc, d| acc + d);
        total / averages.len() as u32
    }
}

/// Format a duration as a human-readable ETA string.
pub fn format_eta(duration: Duration) -> String {
    let total_secs = duration.as_secs();

    if total_secs < 60 {
        format!("{}s", total_secs)
    } else if total_secs < 3600 {
        let mins = total_secs / 60;
        let secs = total_secs % 60;
        if secs > 0 {
            format!("{}m {}s", mins, secs)
        } else {
            format!("{}m", mins)
        }
    } else {
        let hours = total_secs / 3600;
        let mins = (total_secs % 3600) / 60;
        if mins > 0 {
            format!("{}h {}m", hours, mins)
        } else {
            format!("{}h", hours)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::execution_history::ExecutionEntry;
    use std::collections::HashMap;

    fn create_test_history() -> ExecutionHistory {
        let mut entries = Vec::new();

        // Add 3 entries for high confidence
        for i in 0..3 {
            entries.push(ExecutionEntry {
                timestamp: format!("2026-01-{:02}T12:00:00Z", 31 - i),
                task_id: format!("RQ-{:04}", i),
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
        }

        ExecutionHistory {
            version: 1,
            entries,
        }
    }

    #[test]
    fn test_eta_calculator_empty() {
        let calculator = EtaCalculator::empty();
        let mut elapsed = HashMap::new();
        elapsed.insert(ExecutionPhase::Planning, Duration::from_secs(30));

        let eta =
            calculator.calculate_eta("codex", "sonnet", 3, ExecutionPhase::Planning, &elapsed);
        assert!(eta.is_some());
        let estimate = eta.unwrap();
        assert!(!estimate.based_on_history);
        assert_eq!(estimate.confidence, EtaConfidence::Low);
    }

    #[test]
    fn test_eta_calculator_with_history() {
        let history = create_test_history();
        let calculator = EtaCalculator::new(history);
        let mut elapsed = HashMap::new();
        elapsed.insert(ExecutionPhase::Planning, Duration::from_secs(30));

        let eta =
            calculator.calculate_eta("codex", "sonnet", 3, ExecutionPhase::Planning, &elapsed);
        assert!(eta.is_some());
        let estimate = eta.unwrap();
        assert!(estimate.based_on_history);
        // With 3 entries, should be medium confidence
        assert_eq!(estimate.confidence, EtaConfidence::Medium);
    }

    #[test]
    fn test_eta_calculation_first_phase() {
        let history = create_test_history();
        let calculator = EtaCalculator::new(history);
        let mut elapsed = HashMap::new();
        elapsed.insert(ExecutionPhase::Planning, Duration::from_secs(30));

        let eta =
            calculator.calculate_eta("codex", "sonnet", 3, ExecutionPhase::Planning, &elapsed);
        assert!(eta.is_some());
        let estimate = eta.unwrap();

        // Planning avg is 60s, elapsed is 30s, so ~30s remaining
        // Plus Implementation (120s) + Review (30s) = ~180s total
        assert!(estimate.remaining >= Duration::from_secs(150));
    }

    #[test]
    fn test_eta_calculation_second_phase() {
        let history = create_test_history();
        let calculator = EtaCalculator::new(history);
        let mut elapsed = HashMap::new();
        elapsed.insert(ExecutionPhase::Planning, Duration::from_secs(60));
        elapsed.insert(ExecutionPhase::Implementation, Duration::from_secs(60));

        let eta = calculator.calculate_eta(
            "codex",
            "sonnet",
            3,
            ExecutionPhase::Implementation,
            &elapsed,
        );
        assert!(eta.is_some());
        let estimate = eta.unwrap();

        // Implementation avg is 120s, elapsed is 60s, so ~60s remaining
        // Plus Review (30s) = ~90s total
        assert!(estimate.remaining >= Duration::from_secs(60));
        assert!(estimate.remaining <= Duration::from_secs(120));
    }

    #[test]
    fn test_eta_calculation_final_phase() {
        let history = create_test_history();
        let calculator = EtaCalculator::new(history);
        let mut elapsed = HashMap::new();
        elapsed.insert(ExecutionPhase::Planning, Duration::from_secs(60));
        elapsed.insert(ExecutionPhase::Implementation, Duration::from_secs(120));
        elapsed.insert(ExecutionPhase::Review, Duration::from_secs(10));

        let eta = calculator.calculate_eta("codex", "sonnet", 3, ExecutionPhase::Review, &elapsed);
        assert!(eta.is_some());
        let estimate = eta.unwrap();

        // Review avg is 30s, elapsed is 10s, so ~20s remaining
        assert!(estimate.remaining <= Duration::from_secs(30));
    }

    #[test]
    fn test_eta_without_history() {
        let calculator = EtaCalculator::empty();
        let mut elapsed = HashMap::new();
        elapsed.insert(ExecutionPhase::Planning, Duration::from_secs(60));

        let eta =
            calculator.calculate_eta("codex", "sonnet", 3, ExecutionPhase::Planning, &elapsed);
        assert!(eta.is_some());
        let estimate = eta.unwrap();

        // Without history, should use heuristic (current elapsed * remaining phases)
        assert!(!estimate.based_on_history);
        assert!(estimate.remaining > Duration::ZERO);
    }

    #[test]
    fn test_confidence_levels() {
        assert_eq!(EtaConfidence::High.indicator(), "●");
        assert_eq!(EtaConfidence::Medium.indicator(), "◐");
        assert_eq!(EtaConfidence::Low.indicator(), "○");

        assert_eq!(EtaConfidence::High.color_name(), "green");
        assert_eq!(EtaConfidence::Medium.color_name(), "yellow");
        assert_eq!(EtaConfidence::Low.color_name(), "gray");
    }

    #[test]
    fn test_format_eta() {
        assert_eq!(format_eta(Duration::from_secs(30)), "30s");
        assert_eq!(format_eta(Duration::from_secs(90)), "1m 30s");
        assert_eq!(format_eta(Duration::from_secs(60)), "1m");
        assert_eq!(format_eta(Duration::from_secs(3665)), "1h 1m");
        assert_eq!(format_eta(Duration::from_secs(7200)), "2h");
    }

    #[test]
    fn test_single_phase_eta() {
        let history = create_test_history();
        let calculator = EtaCalculator::new(history);
        let mut elapsed = HashMap::new();
        elapsed.insert(ExecutionPhase::Planning, Duration::from_secs(30));

        let eta =
            calculator.calculate_eta("codex", "sonnet", 1, ExecutionPhase::Planning, &elapsed);
        assert!(eta.is_some());
        let estimate = eta.unwrap();

        // Single phase: remaining should be ~30s (60s avg - 30s elapsed)
        assert!(estimate.remaining <= Duration::from_secs(60));
    }

    #[test]
    fn test_two_phase_eta() {
        let history = create_test_history();
        let calculator = EtaCalculator::new(history);
        let mut elapsed = HashMap::new();
        elapsed.insert(ExecutionPhase::Planning, Duration::from_secs(60));
        elapsed.insert(ExecutionPhase::Implementation, Duration::from_secs(60));

        let eta = calculator.calculate_eta(
            "codex",
            "sonnet",
            2,
            ExecutionPhase::Implementation,
            &elapsed,
        );
        assert!(eta.is_some());
        let estimate = eta.unwrap();

        // Two phases: remaining should be ~60s for Implementation
        assert!(estimate.remaining >= Duration::from_secs(30));
        assert!(estimate.remaining <= Duration::from_secs(120));
    }
}
