//! CLI execution timing accumulation for writing execution history.
//!
//! Purpose:
//! - CLI execution timing accumulation for writing execution history.
//!
//! Responsibilities:
//! - Accumulate runner wall-clock durations per execution phase.
//! - Track and validate that runner/model are consistent across all recorded passes.
//! - Build a persistence-ready payload for `crate::execution_history::record_execution`.
//!
//! Not handled here:
//! - Running the runner itself or parsing runner output.
//! - Deciding if a task is actually Done (callers must ensure completion).
//! - Including CI-gate wall time (only runner wall time is recorded).
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - PhaseType::SinglePhase is recorded as ExecutionPhase::Planning (matches ETA phase_count=1).
//! - If multiple runner/model pairs are observed, persistence is skipped.

use crate::commands::run::PhaseType;
use crate::contracts::{Model, Runner};
use crate::progress::ExecutionPhase;
use std::collections::HashMap;
use std::time::Duration;

/// Accumulates execution timings for a single run across all phases.
#[derive(Debug, Default)]
pub(crate) struct RunExecutionTimings {
    canonical: Option<(String, String)>,
    mixed_runner_model: bool,
    phase_durations: HashMap<ExecutionPhase, Duration>,
}

/// Payload ready for persistence to execution history.
#[derive(Debug)]
pub(crate) struct ExecutionHistoryPayload {
    pub runner: String,
    pub model: String,
    pub phase_durations: HashMap<ExecutionPhase, Duration>,
    pub total_duration: Duration,
}

impl RunExecutionTimings {
    /// Record a runner duration for a specific phase.
    ///
    /// This accumulates time into the appropriate phase bucket and tracks
    /// runner/model consistency. Multiple calls for the same phase will
    /// accumulate (e.g., for resume/continue cycles).
    pub(crate) fn record_runner_duration(
        &mut self,
        phase_type: PhaseType,
        runner: &Runner,
        model: &Model,
        duration: Duration,
    ) {
        let phase = match phase_type {
            PhaseType::Planning => ExecutionPhase::Planning,
            PhaseType::Implementation => ExecutionPhase::Implementation,
            PhaseType::Review => ExecutionPhase::Review,
            PhaseType::SinglePhase => ExecutionPhase::Planning,
        };

        let pair = (runner.as_str().to_string(), model.as_str().to_string());
        if let Some(existing) = self.canonical.as_ref() {
            if existing != &pair {
                self.mixed_runner_model = true;
            }
        } else {
            self.canonical = Some(pair);
        }

        let entry = self.phase_durations.entry(phase).or_insert(Duration::ZERO);
        *entry += duration;
    }

    /// Build a persistence payload if runner/model are consistent.
    ///
    /// Returns `None` if:
    /// - Multiple different runner/model pairs were recorded
    /// - No timings were recorded at all
    ///
    /// The `phase_count` parameter filters phases to only those relevant
    /// for the configured phase count (1, 2, or 3).
    pub(crate) fn build_payload(&self, phase_count: u8) -> Option<ExecutionHistoryPayload> {
        if self.mixed_runner_model {
            return None;
        }
        let (runner, model) = self.canonical.clone()?;

        let mut phase_durations = HashMap::new();
        phase_durations.extend(self.phase_durations.iter().map(|(k, v)| (*k, *v)));

        // Filter to only phases relevant for the configured phase_count.
        match phase_count {
            1 => {
                phase_durations.retain(|phase, _| matches!(phase, ExecutionPhase::Planning));
            }
            2 => {
                phase_durations.retain(|phase, _| {
                    matches!(
                        phase,
                        ExecutionPhase::Planning | ExecutionPhase::Implementation
                    )
                });
            }
            _ => {
                phase_durations.retain(|phase, _| {
                    matches!(
                        phase,
                        ExecutionPhase::Planning
                            | ExecutionPhase::Implementation
                            | ExecutionPhase::Review
                    )
                });
            }
        }

        let total_duration = phase_durations
            .values()
            .copied()
            .fold(Duration::ZERO, |acc, d| acc + d);

        Some(ExecutionHistoryPayload {
            runner,
            model,
            phase_durations,
            total_duration,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_record_single_phase() {
        let mut timings = RunExecutionTimings::default();
        timings.record_runner_duration(
            PhaseType::Planning,
            &Runner::Codex,
            &Model::Gpt53,
            Duration::from_secs(60),
        );

        let payload = timings.build_payload(2).unwrap();
        assert_eq!(payload.runner, "codex");
        assert_eq!(payload.model, "gpt-5.3");
        assert_eq!(
            payload.phase_durations.get(&ExecutionPhase::Planning),
            Some(&Duration::from_secs(60))
        );
        assert_eq!(payload.total_duration, Duration::from_secs(60));
    }

    #[test]
    fn test_accumulate_multiple_phases() {
        let mut timings = RunExecutionTimings::default();
        timings.record_runner_duration(
            PhaseType::Planning,
            &Runner::Codex,
            &Model::Gpt53,
            Duration::from_secs(60),
        );
        timings.record_runner_duration(
            PhaseType::Implementation,
            &Runner::Codex,
            &Model::Gpt53,
            Duration::from_secs(120),
        );

        let payload = timings.build_payload(2).unwrap();
        assert_eq!(payload.total_duration, Duration::from_secs(180));
        assert_eq!(
            payload.phase_durations.get(&ExecutionPhase::Planning),
            Some(&Duration::from_secs(60))
        );
        assert_eq!(
            payload.phase_durations.get(&ExecutionPhase::Implementation),
            Some(&Duration::from_secs(120))
        );
    }

    #[test]
    fn test_accumulate_same_phase_multiple_times() {
        let mut timings = RunExecutionTimings::default();
        timings.record_runner_duration(
            PhaseType::Implementation,
            &Runner::Codex,
            &Model::Gpt53,
            Duration::from_secs(60),
        );
        timings.record_runner_duration(
            PhaseType::Implementation,
            &Runner::Codex,
            &Model::Gpt53,
            Duration::from_secs(30),
        );

        let payload = timings.build_payload(2).unwrap();
        assert_eq!(
            payload.phase_durations.get(&ExecutionPhase::Implementation),
            Some(&Duration::from_secs(90))
        );
    }

    #[test]
    fn test_single_phase_maps_to_planning() {
        let mut timings = RunExecutionTimings::default();
        timings.record_runner_duration(
            PhaseType::SinglePhase,
            &Runner::Claude,
            &Model::Gpt53,
            Duration::from_secs(45),
        );

        let payload = timings.build_payload(1).unwrap();
        assert_eq!(
            payload.phase_durations.get(&ExecutionPhase::Planning),
            Some(&Duration::from_secs(45))
        );
    }

    #[test]
    fn test_mixed_runner_model_returns_none() {
        let mut timings = RunExecutionTimings::default();
        timings.record_runner_duration(
            PhaseType::Planning,
            &Runner::Codex,
            &Model::Gpt53,
            Duration::from_secs(60),
        );
        timings.record_runner_duration(
            PhaseType::Implementation,
            &Runner::Claude, // Different runner!
            &Model::Gpt53,
            Duration::from_secs(120),
        );

        assert!(timings.build_payload(2).is_none());
    }

    #[test]
    fn test_mixed_model_returns_none() {
        let mut timings = RunExecutionTimings::default();
        timings.record_runner_duration(
            PhaseType::Planning,
            &Runner::Codex,
            &Model::Gpt53,
            Duration::from_secs(60),
        );
        timings.record_runner_duration(
            PhaseType::Implementation,
            &Runner::Codex,
            &Model::Gpt54, // Different model!
            Duration::from_secs(120),
        );

        assert!(timings.build_payload(2).is_none());
    }

    #[test]
    fn test_empty_timings_returns_none() {
        let timings = RunExecutionTimings::default();
        assert!(timings.build_payload(2).is_none());
    }

    #[test]
    fn test_phase_count_filtering() {
        let mut timings = RunExecutionTimings::default();
        timings.record_runner_duration(
            PhaseType::Planning,
            &Runner::Codex,
            &Model::Gpt53,
            Duration::from_secs(60),
        );
        timings.record_runner_duration(
            PhaseType::Implementation,
            &Runner::Codex,
            &Model::Gpt53,
            Duration::from_secs(120),
        );
        timings.record_runner_duration(
            PhaseType::Review,
            &Runner::Codex,
            &Model::Gpt53,
            Duration::from_secs(30),
        );

        // phase_count=1 should only keep Planning
        let payload1 = timings.build_payload(1).unwrap();
        assert_eq!(payload1.phase_durations.len(), 1);
        assert!(
            payload1
                .phase_durations
                .contains_key(&ExecutionPhase::Planning)
        );

        // phase_count=2 should keep Planning and Implementation
        let payload2 = timings.build_payload(2).unwrap();
        assert_eq!(payload2.phase_durations.len(), 2);
        assert!(
            payload2
                .phase_durations
                .contains_key(&ExecutionPhase::Planning)
        );
        assert!(
            payload2
                .phase_durations
                .contains_key(&ExecutionPhase::Implementation)
        );

        // phase_count=3 should keep all three
        let payload3 = timings.build_payload(3).unwrap();
        assert_eq!(payload3.phase_durations.len(), 3);
        assert!(
            payload3
                .phase_durations
                .contains_key(&ExecutionPhase::Planning)
        );
        assert!(
            payload3
                .phase_durations
                .contains_key(&ExecutionPhase::Implementation)
        );
        assert!(
            payload3
                .phase_durations
                .contains_key(&ExecutionPhase::Review)
        );
    }
}
