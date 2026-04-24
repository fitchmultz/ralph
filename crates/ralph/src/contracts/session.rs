//! Session state contract for crash recovery.
//!
//! Purpose:
//! - Session state contract for crash recovery.
//!
//! Responsibilities:
//! - Define the session state schema for run loop recovery.
//! - Provide serialization/deserialization for session persistence.
//!
//! Not handled here:
//! - Session persistence operations (see crate::session).
//! - Session validation logic (see crate::session).
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Session state is written atomically to prevent corruption.
//! - Timestamps are RFC3339 UTC format.
//! - Per-phase settings are display-only; crash recovery recomputes from CLI+config+task.

use crate::constants::versions::SESSION_STATE_VERSION;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{ReasoningEffort, Runner};

/// Per-phase settings persisted for display/logging purposes.
///
/// These fields are informational only - crash recovery recomputes settings
/// from CLI flags, config, and task overrides to ensure consistency.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PhaseSettingsSnapshot {
    /// Runner for this phase
    pub runner: Runner,
    /// Model for this phase
    pub model: String,
    /// Reasoning effort for this phase (if applicable)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_effort: Option<ReasoningEffort>,
}

/// Session state persisted to enable crash recovery.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SessionState {
    /// Schema version for forward compatibility.
    pub version: u32,

    /// Unique session ID (UUID v4) for this run session.
    pub session_id: String,

    /// The task currently being executed.
    pub task_id: String,

    /// When the session/run started (RFC3339 UTC).
    pub run_started_at: String,

    /// When the session state was last updated (RFC3339 UTC).
    pub last_updated_at: String,

    /// Total number of iterations planned for the current task.
    pub iterations_planned: u8,

    /// Number of iterations completed so far.
    pub iterations_completed: u8,

    /// Current phase being executed (1, 2, or 3).
    pub current_phase: u8,

    /// Runner being used for this session.
    pub runner: Runner,

    /// Model being used for this session.
    pub model: String,

    /// Number of tasks completed in this loop session (for loop progress tracking).
    pub tasks_completed_in_loop: u32,

    /// Maximum tasks to run in this loop (0 = no limit).
    pub max_tasks: u32,

    /// Git HEAD commit at session start (for advanced recovery validation).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub git_head_commit: Option<String>,

    /// Phase 1 settings (planning) - display/logging only.
    /// Crash recovery recomputes from CLI+config+task.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub phase1_settings: Option<PhaseSettingsSnapshot>,

    /// Phase 2 settings (implementation) - display/logging only.
    /// Crash recovery recomputes from CLI+config+task.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub phase2_settings: Option<PhaseSettingsSnapshot>,

    /// Phase 3 settings (review) - display/logging only.
    /// Crash recovery recomputes from CLI+config+task.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub phase3_settings: Option<PhaseSettingsSnapshot>,
}

impl SessionState {
    /// Create a new session state for the given task.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        session_id: String,
        task_id: String,
        run_started_at: String,
        iterations_planned: u8,
        runner: Runner,
        model: String,
        max_tasks: u32,
        git_head_commit: Option<String>,
        phase_settings: Option<(
            PhaseSettingsSnapshot,
            PhaseSettingsSnapshot,
            PhaseSettingsSnapshot,
        )>,
    ) -> Self {
        let (phase1_settings, phase2_settings, phase3_settings) = phase_settings
            .map(|(p1, p2, p3)| (Some(p1), Some(p2), Some(p3)))
            .unwrap_or((None, None, None));

        Self {
            version: SESSION_STATE_VERSION,
            session_id,
            task_id,
            run_started_at: run_started_at.clone(),
            last_updated_at: run_started_at,
            iterations_planned,
            iterations_completed: 0,
            current_phase: 1,
            runner,
            model,
            tasks_completed_in_loop: 0,
            max_tasks,
            git_head_commit,
            phase1_settings,
            phase2_settings,
            phase3_settings,
        }
    }

    /// Update the session after iteration completion.
    pub fn mark_iteration_complete(&mut self, completed_at: String) {
        self.iterations_completed += 1;
        self.last_updated_at = completed_at;
    }

    /// Update the session after phase completion.
    pub fn set_phase(&mut self, phase: u8, updated_at: String) {
        self.current_phase = phase;
        self.last_updated_at = updated_at;
    }

    /// Update the session after task completion.
    pub fn mark_task_complete(&mut self, updated_at: String) {
        self.tasks_completed_in_loop += 1;
        self.last_updated_at = updated_at;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_session() -> SessionState {
        SessionState::new(
            "test-session-id".to_string(),
            "RQ-0001".to_string(),
            "2026-01-30T00:00:00.000000000Z".to_string(),
            2,
            Runner::Claude,
            "sonnet".to_string(),
            10,
            Some("abc123".to_string()),
            None, // phase_settings
        )
    }

    #[test]
    fn session_new_sets_defaults() {
        let session = test_session();

        assert_eq!(session.version, SESSION_STATE_VERSION);
        assert_eq!(session.session_id, "test-session-id");
        assert_eq!(session.task_id, "RQ-0001");
        assert_eq!(session.iterations_planned, 2);
        assert_eq!(session.iterations_completed, 0);
        assert_eq!(session.current_phase, 1);
        assert_eq!(session.tasks_completed_in_loop, 0);
        assert_eq!(session.max_tasks, 10);
        assert_eq!(session.git_head_commit, Some("abc123".to_string()));
    }

    #[test]
    fn session_mark_iteration_complete_increments_count() {
        let mut session = test_session();

        session.mark_iteration_complete("2026-01-30T00:01:00.000000000Z".to_string());

        assert_eq!(session.iterations_completed, 1);
        assert_eq!(session.last_updated_at, "2026-01-30T00:01:00.000000000Z");
    }

    #[test]
    fn session_set_phase_updates_phase() {
        let mut session = test_session();

        session.set_phase(2, "2026-01-30T00:02:00.000000000Z".to_string());

        assert_eq!(session.current_phase, 2);
        assert_eq!(session.last_updated_at, "2026-01-30T00:02:00.000000000Z");
    }

    #[test]
    fn session_mark_task_complete_increments_count() {
        let mut session = test_session();

        session.mark_task_complete("2026-01-30T00:03:00.000000000Z".to_string());

        assert_eq!(session.tasks_completed_in_loop, 1);
        assert_eq!(session.last_updated_at, "2026-01-30T00:03:00.000000000Z");
    }

    #[test]
    fn session_serialization_roundtrip() {
        let session = test_session();

        let json = serde_json::to_string(&session).expect("serialize");
        let deserialized: SessionState = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(deserialized.session_id, session.session_id);
        assert_eq!(deserialized.task_id, session.task_id);
        assert_eq!(deserialized.iterations_planned, session.iterations_planned);
        assert_eq!(deserialized.runner, session.runner);
        assert_eq!(deserialized.model, session.model);
    }

    #[test]
    fn session_deserialization_ignores_optional_git_commit_when_none() {
        let session = SessionState::new(
            "test-id".to_string(),
            "RQ-0001".to_string(),
            "2026-01-30T00:00:00.000000000Z".to_string(),
            1,
            Runner::Claude,
            "sonnet".to_string(),
            0,
            None,
            None, // phase_settings
        );

        let json = serde_json::to_string(&session).expect("serialize");
        assert!(!json.contains("git_head_commit"));

        let deserialized: SessionState = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(deserialized.git_head_commit, None);
    }

    #[test]
    fn session_new_with_phase_settings() {
        let phase_settings = (
            PhaseSettingsSnapshot {
                runner: Runner::Claude,
                model: "sonnet".to_string(),
                reasoning_effort: None,
            },
            PhaseSettingsSnapshot {
                runner: Runner::Codex,
                model: "o3-mini".to_string(),
                reasoning_effort: Some(ReasoningEffort::High),
            },
            PhaseSettingsSnapshot {
                runner: Runner::Claude,
                model: "haiku".to_string(),
                reasoning_effort: None,
            },
        );

        let session = SessionState::new(
            "test-id".to_string(),
            "RQ-0001".to_string(),
            "2026-01-30T00:00:00.000000000Z".to_string(),
            1,
            Runner::Claude,
            "sonnet".to_string(),
            0,
            None,
            Some(phase_settings),
        );

        assert!(session.phase1_settings.is_some());
        assert!(session.phase2_settings.is_some());
        assert!(session.phase3_settings.is_some());

        let p1 = session.phase1_settings.unwrap();
        assert_eq!(p1.runner, Runner::Claude);
        assert_eq!(p1.model, "sonnet");
        assert_eq!(p1.reasoning_effort, None);

        let p2 = session.phase2_settings.unwrap();
        assert_eq!(p2.runner, Runner::Codex);
        assert_eq!(p2.model, "o3-mini");
        assert_eq!(p2.reasoning_effort, Some(ReasoningEffort::High));

        let p3 = session.phase3_settings.unwrap();
        assert_eq!(p3.runner, Runner::Claude);
        assert_eq!(p3.model, "haiku");
        assert_eq!(p3.reasoning_effort, None);
    }

    #[test]
    fn session_serialization_with_phase_settings() {
        let phase_settings = (
            PhaseSettingsSnapshot {
                runner: Runner::Claude,
                model: "sonnet".to_string(),
                reasoning_effort: None,
            },
            PhaseSettingsSnapshot {
                runner: Runner::Codex,
                model: "o3-mini".to_string(),
                reasoning_effort: Some(ReasoningEffort::Medium),
            },
            PhaseSettingsSnapshot {
                runner: Runner::Claude,
                model: "haiku".to_string(),
                reasoning_effort: None,
            },
        );

        let session = SessionState::new(
            "test-id".to_string(),
            "RQ-0001".to_string(),
            "2026-01-30T00:00:00.000000000Z".to_string(),
            1,
            Runner::Claude,
            "sonnet".to_string(),
            0,
            None,
            Some(phase_settings),
        );

        let json = serde_json::to_string(&session).expect("serialize");
        let deserialized: SessionState = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(deserialized.phase1_settings, session.phase1_settings);
        assert_eq!(deserialized.phase2_settings, session.phase2_settings);
        assert_eq!(deserialized.phase3_settings, session.phase3_settings);
    }

    #[test]
    fn session_deserialization_backward_compatible_without_phase_settings() {
        // Simulate old session JSON without phase settings
        // Note: runner uses kebab-case serialization
        let json = r#"{
            "version": 1,
            "session_id": "test-id",
            "task_id": "RQ-0001",
            "run_started_at": "2026-01-30T00:00:00.000000000Z",
            "last_updated_at": "2026-01-30T00:00:00.000000000Z",
            "iterations_planned": 1,
            "iterations_completed": 0,
            "current_phase": 1,
            "runner": "claude",
            "model": "sonnet",
            "tasks_completed_in_loop": 0,
            "max_tasks": 0
        }"#;

        let session: SessionState = serde_json::from_str(json).expect("deserialize old format");
        assert_eq!(session.phase1_settings, None);
        assert_eq!(session.phase2_settings, None);
        assert_eq!(session.phase3_settings, None);
    }

    #[test]
    fn session_serialization_skips_none_phase_settings() {
        let session = SessionState::new(
            "test-id".to_string(),
            "RQ-0001".to_string(),
            "2026-01-30T00:00:00.000000000Z".to_string(),
            1,
            Runner::Claude,
            "sonnet".to_string(),
            0,
            None,
            None, // no phase settings
        );

        let json = serde_json::to_string(&session).expect("serialize");
        // Phase settings fields should not be present when None
        assert!(!json.contains("phase1_settings"));
        assert!(!json.contains("phase2_settings"));
        assert!(!json.contains("phase3_settings"));
    }
}
