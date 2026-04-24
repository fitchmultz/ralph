//! Execution-history ETA helpers for stats reports.
//!
//! Purpose:
//! - Execution-history ETA helpers for stats reports.
//!
//! Responsibilities:
//! - Resolve runner/model settings for a stats report ETA lookup.
//! - Read ETA history samples from cache and convert them into report output.
//!
//! Not handled here:
//! - Queue summary or breakdown calculation.
//! - Text or JSON rendering.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - ETA remains optional when there are no samples for the resolved runner/model/phases key.

use std::path::Path;

use crate::contracts::AgentConfig;
use crate::eta_calculator::{EtaCalculator, format_eta};
use crate::runner::resolve_agent_settings;

use super::model::ExecutionHistoryEtaReport;

pub(super) fn build_execution_history_eta(
    resolved_config: &AgentConfig,
    cache_dir: &Path,
) -> Option<ExecutionHistoryEtaReport> {
    let empty_cli_patch = crate::contracts::RunnerCliOptionsPatch::default();
    let settings =
        resolve_agent_settings(None, None, None, &empty_cli_patch, None, resolved_config).ok()?;

    let phase_count = resolved_config.phases.unwrap_or(3);
    let calculator = EtaCalculator::load(cache_dir);
    let estimate = calculator.estimate_new_task_total(
        settings.runner.as_str(),
        settings.model.as_str(),
        phase_count,
    )?;

    let sample_count = calculator.count_entries_for_key(
        settings.runner.as_str(),
        settings.model.as_str(),
        phase_count,
    );

    let confidence = match estimate.confidence {
        crate::eta_calculator::EtaConfidence::High => "high",
        crate::eta_calculator::EtaConfidence::Medium => "medium",
        crate::eta_calculator::EtaConfidence::Low => "low",
    };

    Some(ExecutionHistoryEtaReport {
        runner: settings.runner.as_str().to_string(),
        model: settings.model.as_str().to_string(),
        phase_count,
        sample_count,
        estimated_total_seconds: estimate.remaining.as_secs(),
        estimated_total_human: format_eta(estimate.remaining),
        confidence: confidence.to_string(),
    })
}
