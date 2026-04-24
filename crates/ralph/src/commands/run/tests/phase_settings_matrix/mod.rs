//! Per-phase settings resolution matrix test hub.
//!
//! Purpose:
//! - Per-phase settings resolution matrix test hub.
//!
//! Responsibilities:
//! - Group phase-settings matrix coverage by behavior area.
//! - Share common imports and fixtures across focused matrix submodules.
//!
//! Not handled here:
//! - Unrelated run-command suites.
//! - Production phase-settings resolution.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - This module stays a thin hub with behavior-grouped companions.
//! - Matrix scenarios continue to exercise `resolve_phase_settings_matrix` end to end.

use super::{test_config_agent, test_overrides_with_phases, test_task_agent};
use crate::agent::AgentOverrides;
use crate::contracts::{
    Model, ModelEffort, PhaseOverrideConfig, PhaseOverrides, ReasoningEffort, Runner, TaskAgent,
};
use crate::runner::resolve_phase_settings_matrix;

mod execution_modes;
mod integration;
mod model_defaults;
mod precedence;
mod validation;
