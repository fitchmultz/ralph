//! Execution phases and small progress-related contracts.
//!
//! Responsibilities:
//! - Define `ExecutionPhase`, a stable enum used across run history and ETA estimation.
//! - Provide small, display-oriented helpers (phase name/number/icon) without owning rendering.
//!
//! Not handled here:
//! - Rendering progress bars/spinners (CLI or GUI).
//! - Persisting execution history (see `crate::execution_history`).
//! - ETA heuristics (see `crate::eta_calculator`).
//!
//! Invariants/assumptions:
//! - Phase numbering is stable: planning=1, implementation=2, review=3, complete=0.
//! - Serialization format is snake_case for stable on-disk contracts.

use serde::{Deserialize, Serialize};

/// Execution phases for multi-phase task workflows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionPhase {
    /// Phase 1: Planning and analysis.
    Planning,
    /// Phase 2: Implementation and CI.
    Implementation,
    /// Phase 3: Review and completion.
    Review,
    /// Execution completed.
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

    /// Returns an icon representation of the phase.
    ///
    /// This is used for terminal output; GUIs should render their own icons.
    pub fn icon(&self) -> &'static str {
        match self {
            ExecutionPhase::Planning => "▶",
            ExecutionPhase::Implementation => "⚙",
            ExecutionPhase::Review => "👁",
            ExecutionPhase::Complete => "✓",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn execution_phase_as_str() {
        assert_eq!(ExecutionPhase::Planning.as_str(), "Planning");
        assert_eq!(ExecutionPhase::Implementation.as_str(), "Implementation");
        assert_eq!(ExecutionPhase::Review.as_str(), "Review");
        assert_eq!(ExecutionPhase::Complete.as_str(), "Complete");
    }

    #[test]
    fn execution_phase_number() {
        assert_eq!(ExecutionPhase::Planning.phase_number(), 1);
        assert_eq!(ExecutionPhase::Implementation.phase_number(), 2);
        assert_eq!(ExecutionPhase::Review.phase_number(), 3);
        assert_eq!(ExecutionPhase::Complete.phase_number(), 0);
    }
}
