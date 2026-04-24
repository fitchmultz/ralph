//! Phase override configuration for per-phase runner/model/reasoning settings.
//!
//! Purpose:
//! - Phase override configuration for per-phase runner/model/reasoning settings.
//!
//! Responsibilities:
//! - Define phase override structs and merge behavior.
//!
//! Not handled here:
//! - Phase execution logic (see `crate::commands::run::phases` module).
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/Assumptions:
//! - Keep behavior aligned with Ralph's canonical CLI, machine-contract, and queue semantics.

use crate::contracts::model::{Model, ReasoningEffort};
use crate::contracts::runner::Runner;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Per-phase configuration overrides for runner, model, and reasoning effort.
///
/// All fields are optional to support leaf-wise merging:
/// - `Some(value)` overrides the parent config
/// - `None` means "inherit from parent"
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct PhaseOverrideConfig {
    /// Runner to use for this phase (overrides global agent.runner)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runner: Option<Runner>,

    /// Model to use for this phase (overrides global agent.model)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<Model>,

    /// Reasoning effort for this phase (overrides global agent.reasoning_effort)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_effort: Option<ReasoningEffort>,
}

impl PhaseOverrideConfig {
    /// Leaf-wise merge: other.Some overrides self, other.None preserves self
    pub fn merge_from(&mut self, other: Self) {
        if other.runner.is_some() {
            self.runner = other.runner;
        }
        if other.model.is_some() {
            self.model = other.model;
        }
        if other.reasoning_effort.is_some() {
            self.reasoning_effort = other.reasoning_effort;
        }
    }
}

/// Phase overrides container for Phase 1/2/3 execution.
///
/// Per-phase configuration for Phase 1/2/3 execution.
///
/// Invariants/assumptions:
/// - Overrides are defined per phase only; there is no shared `defaults` layer inside
///   `agent.phase_overrides`. Use global `agent.runner` / `agent.model` /
///   `agent.reasoning_effort` for shared defaults.
/// - Merging is leaf-wise: `Some(value)` overrides, `None` inherits.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct PhaseOverrides {
    /// Phase 1 specific overrides
    #[serde(skip_serializing_if = "Option::is_none")]
    pub phase1: Option<PhaseOverrideConfig>,

    /// Phase 2 specific overrides
    #[serde(skip_serializing_if = "Option::is_none")]
    pub phase2: Option<PhaseOverrideConfig>,

    /// Phase 3 specific overrides
    #[serde(skip_serializing_if = "Option::is_none")]
    pub phase3: Option<PhaseOverrideConfig>,
}

impl PhaseOverrides {
    /// Merge other into self following leaf-wise semantics:
    /// Merge each specific phase override
    pub fn merge_from(&mut self, other: Self) {
        // Merge phase1
        match (&mut self.phase1, other.phase1) {
            (Some(existing), Some(new)) => existing.merge_from(new),
            (None, Some(new)) => self.phase1 = Some(new),
            _ => {}
        }

        // Merge phase2
        match (&mut self.phase2, other.phase2) {
            (Some(existing), Some(new)) => existing.merge_from(new),
            (None, Some(new)) => self.phase2 = Some(new),
            _ => {}
        }

        // Merge phase3
        match (&mut self.phase3, other.phase3) {
            (Some(existing), Some(new)) => existing.merge_from(new),
            (None, Some(new)) => self.phase3 = Some(new),
            _ => {}
        }
    }
}
