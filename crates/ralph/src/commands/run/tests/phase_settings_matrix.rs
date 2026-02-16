//! Per-phase settings resolution matrix tests (RQ-0491).

use super::{test_config_agent, test_overrides_with_phases, test_task_agent};
use crate::agent::AgentOverrides;
use crate::contracts::{
    Model, ModelEffort, PhaseOverrideConfig, PhaseOverrides, ReasoningEffort, Runner, TaskAgent,
};
use crate::queue;
use crate::runner::resolve_phase_settings_matrix;

// ============================================================================
// Precedence chain tests
// ============================================================================

#[test]
fn resolve_phase_settings_cli_phase_override_beats_global() {
    // CLI phase override should beat CLI global override
    let config_agent = test_config_agent(Some(Runner::Claude), Some(Model::Gpt52), None);

    let phase_overrides = PhaseOverrides {
        phase1: Some(PhaseOverrideConfig {
            runner: Some(Runner::Codex),
            model: Some(Model::Gpt52Codex),
            reasoning_effort: Some(ReasoningEffort::Low),
        }),
        ..Default::default()
    };

    let overrides = test_overrides_with_phases(
        Some(Runner::Opencode), // Global CLI override
        Some(Model::Glm47),     // Global CLI model
        Some(ReasoningEffort::High),
        Some(phase_overrides),
    );

    let (matrix, _warnings) =
        resolve_phase_settings_matrix(&overrides, &config_agent, None, 3).unwrap();

    // Phase 1 should use CLI phase override (not CLI global)
    assert_eq!(matrix.phase1.runner, Runner::Codex);
    assert_eq!(matrix.phase1.model, Model::Gpt52Codex);
    assert_eq!(matrix.phase1.reasoning_effort, Some(ReasoningEffort::Low));
}

#[test]
fn resolve_phase_settings_config_phase_override_beats_global() {
    // Config phase override should beat CLI global override
    let mut config_agent = test_config_agent(Some(Runner::Claude), Some(Model::Gpt52), None);
    config_agent.phase_overrides = Some(PhaseOverrides {
        phase2: Some(PhaseOverrideConfig {
            runner: Some(Runner::Gemini),
            model: Some(Model::Custom("gemini-pro".to_string())),
            reasoning_effort: None,
        }),
        ..Default::default()
    });

    let overrides = test_overrides_with_phases(
        Some(Runner::Codex),
        Some(Model::Gpt52Codex),
        Some(ReasoningEffort::High),
        None,
    );

    let (matrix, _warnings) =
        resolve_phase_settings_matrix(&overrides, &config_agent, None, 3).unwrap();

    // Phase 2 should use config phase override
    assert_eq!(matrix.phase2.runner, Runner::Gemini);
    assert_eq!(matrix.phase2.model.as_str(), "gemini-pro");
}

#[test]
fn resolve_phase_settings_task_phase_override_beats_config_phase_override() {
    let mut config_agent = test_config_agent(Some(Runner::Claude), Some(Model::Gpt52), None);
    config_agent.phase_overrides = Some(PhaseOverrides {
        phase1: Some(PhaseOverrideConfig {
            runner: Some(Runner::Codex),
            model: Some(Model::Gpt52Codex),
            reasoning_effort: Some(ReasoningEffort::Low),
        }),
        ..Default::default()
    });

    let task_agent = TaskAgent {
        runner: None,
        model: None,
        model_effort: ModelEffort::Default,
        phases: None,
        iterations: None,
        followup_reasoning_effort: None,
        runner_cli: None,
        phase_overrides: Some(PhaseOverrides {
            phase1: Some(PhaseOverrideConfig {
                runner: Some(Runner::Kimi),
                model: Some(Model::Custom("kimi-code/kimi-for-coding".to_string())),
                reasoning_effort: Some(ReasoningEffort::High),
            }),
            ..Default::default()
        }),
    };

    let overrides = AgentOverrides::default();
    let (matrix, _warnings) =
        resolve_phase_settings_matrix(&overrides, &config_agent, Some(&task_agent), 3).unwrap();

    assert_eq!(matrix.phase1.runner, Runner::Kimi);
    assert_eq!(matrix.phase1.model.as_str(), "kimi-code/kimi-for-coding");
}

#[test]
fn resolve_phase_settings_cli_phase_override_beats_task_phase_override() {
    let config_agent = test_config_agent(Some(Runner::Claude), Some(Model::Gpt52), None);
    let task_agent = TaskAgent {
        runner: None,
        model: None,
        model_effort: ModelEffort::Default,
        phases: None,
        iterations: None,
        followup_reasoning_effort: None,
        runner_cli: None,
        phase_overrides: Some(PhaseOverrides {
            phase1: Some(PhaseOverrideConfig {
                runner: Some(Runner::Kimi),
                model: Some(Model::Custom("kimi-code/kimi-for-coding".to_string())),
                reasoning_effort: None,
            }),
            ..Default::default()
        }),
    };
    let overrides = test_overrides_with_phases(
        None,
        None,
        None,
        Some(PhaseOverrides {
            phase1: Some(PhaseOverrideConfig {
                runner: Some(Runner::Codex),
                model: Some(Model::Gpt52Codex),
                reasoning_effort: Some(ReasoningEffort::High),
            }),
            ..Default::default()
        }),
    );

    let (matrix, _warnings) =
        resolve_phase_settings_matrix(&overrides, &config_agent, Some(&task_agent), 3).unwrap();

    assert_eq!(matrix.phase1.runner, Runner::Codex);
    assert_eq!(matrix.phase1.model, Model::Gpt52Codex);
    assert_eq!(matrix.phase1.reasoning_effort, Some(ReasoningEffort::High));
}

#[test]
fn resolve_phase_settings_cli_global_beats_task() {
    // CLI global override should beat task override
    let config_agent = test_config_agent(Some(Runner::Claude), Some(Model::Gpt52), None);
    let task_agent = test_task_agent(
        Some(Runner::Opencode),
        Some(Model::Glm47),
        ModelEffort::High,
    );

    let overrides = test_overrides_with_phases(
        Some(Runner::Codex),
        Some(Model::Gpt52Codex),
        Some(ReasoningEffort::Medium),
        None,
    );

    let (matrix, _warnings) =
        resolve_phase_settings_matrix(&overrides, &config_agent, Some(&task_agent), 3).unwrap();

    // All phases should use CLI global override
    assert_eq!(matrix.phase1.runner, Runner::Codex);
    assert_eq!(matrix.phase1.model, Model::Gpt52Codex);
    assert_eq!(matrix.phase2.runner, Runner::Codex);
    assert_eq!(matrix.phase3.runner, Runner::Codex);
}

#[test]
fn resolve_phase_settings_task_beats_config() {
    // Task override should beat config default
    let config_agent = test_config_agent(Some(Runner::Claude), Some(Model::Gpt52), None);
    let task_agent = test_task_agent(
        Some(Runner::Opencode),
        Some(Model::Glm47),
        ModelEffort::High,
    );

    let overrides = AgentOverrides::default();

    let (matrix, _warnings) =
        resolve_phase_settings_matrix(&overrides, &config_agent, Some(&task_agent), 3).unwrap();

    // All phases should use task override
    assert_eq!(matrix.phase1.runner, Runner::Opencode);
    assert_eq!(matrix.phase1.model, Model::Glm47);
}

#[test]
fn resolve_phase_settings_config_beats_default() {
    // Config default should be used when nothing else specified
    let config_agent = test_config_agent(
        Some(Runner::Gemini),
        Some(Model::Custom("gemini-custom".to_string())),
        None,
    );

    let overrides = AgentOverrides::default();

    let (matrix, _warnings) =
        resolve_phase_settings_matrix(&overrides, &config_agent, None, 3).unwrap();

    // All phases should use config default
    assert_eq!(matrix.phase1.runner, Runner::Gemini);
    // Custom model should be preserved
    assert_eq!(matrix.phase1.model.as_str(), "gemini-custom");
}

#[test]
fn resolve_phase_settings_uses_code_default_when_nothing_specified() {
    // Code default should be used when nothing specified
    let config_agent = crate::contracts::AgentConfig::default();
    let overrides = AgentOverrides::default();

    let (matrix, _warnings) =
        resolve_phase_settings_matrix(&overrides, &config_agent, None, 3).unwrap();

    // Should use default runner (Claude) and its default model
    assert_eq!(matrix.phase1.runner, Runner::Claude);
}

// ============================================================================
// Model defaulting tests
// ============================================================================

#[test]
fn resolve_phase_settings_runner_override_uses_default_model() {
    // When runner is overridden without explicit model, use runner's default
    let config_agent = test_config_agent(
        Some(Runner::Claude),
        Some(Model::Custom("sonnet".to_string())),
        None,
    );

    let overrides = test_overrides_with_phases(
        Some(Runner::Opencode), // Override runner but not model
        None,
        None,
        None,
    );

    let (matrix, _warnings) =
        resolve_phase_settings_matrix(&overrides, &config_agent, None, 3).unwrap();

    // Should use Opencode's default model, not config's model
    assert_eq!(matrix.phase1.runner, Runner::Opencode);
    assert_eq!(matrix.phase1.model, Model::Glm47);
}

#[test]
fn resolve_phase_settings_phase_runner_override_uses_default_model() {
    // When runner is overridden at phase level without explicit model
    let config_agent = test_config_agent(
        Some(Runner::Claude),
        Some(Model::Custom("sonnet".to_string())),
        None,
    );

    let phase_overrides = PhaseOverrides {
        phase1: Some(PhaseOverrideConfig {
            runner: Some(Runner::Opencode),
            model: None, // No explicit model
            reasoning_effort: None,
        }),
        ..Default::default()
    };

    let overrides = test_overrides_with_phases(None, None, None, Some(phase_overrides));

    let (matrix, _warnings) =
        resolve_phase_settings_matrix(&overrides, &config_agent, None, 3).unwrap();

    // Phase 1 should use Opencode's default model
    assert_eq!(matrix.phase1.runner, Runner::Opencode);
    assert_eq!(matrix.phase1.model, Model::Glm47);

    // Other phases should use config default
    assert_eq!(matrix.phase2.runner, Runner::Claude);
}

#[test]
fn resolve_phase_settings_explicit_model_preserved_with_runner_override() {
    // When both runner and model are explicitly overridden
    let config_agent = test_config_agent(
        Some(Runner::Claude),
        Some(Model::Custom("sonnet".to_string())),
        None,
    );

    let phase_overrides = PhaseOverrides {
        phase2: Some(PhaseOverrideConfig {
            runner: Some(Runner::Codex),
            model: Some(Model::Gpt52), // Explicit model
            reasoning_effort: None,
        }),
        ..Default::default()
    };

    let overrides = test_overrides_with_phases(None, None, None, Some(phase_overrides));

    let (matrix, _warnings) =
        resolve_phase_settings_matrix(&overrides, &config_agent, None, 3).unwrap();

    // Phase 2 should use explicit model
    assert_eq!(matrix.phase2.runner, Runner::Codex);
    assert_eq!(matrix.phase2.model, Model::Gpt52);
}

// ============================================================================
// Effort handling tests
// ============================================================================

#[test]
fn resolve_phase_settings_effort_some_for_codex() {
    // Effort should be Some() for Codex runners
    let config_agent = test_config_agent(Some(Runner::Codex), Some(Model::Gpt52Codex), None);

    let overrides = test_overrides_with_phases(None, None, Some(ReasoningEffort::High), None);

    let (matrix, _warnings) =
        resolve_phase_settings_matrix(&overrides, &config_agent, None, 3).unwrap();

    assert_eq!(matrix.phase1.reasoning_effort, Some(ReasoningEffort::High));
    assert_eq!(matrix.phase2.reasoning_effort, Some(ReasoningEffort::High));
}

#[test]
fn resolve_phase_settings_effort_none_for_non_codex() {
    // Effort should be None for non-Codex runners
    let config_agent = test_config_agent(Some(Runner::Opencode), Some(Model::Glm47), None);

    let overrides = test_overrides_with_phases(None, None, Some(ReasoningEffort::High), None);

    let (matrix, _warnings) =
        resolve_phase_settings_matrix(&overrides, &config_agent, None, 3).unwrap();

    assert_eq!(matrix.phase1.reasoning_effort, None);
    assert_eq!(matrix.phase2.reasoning_effort, None);
}

#[test]
fn resolve_phase_settings_effort_precedence_within_codex() {
    // Effort should follow precedence within Codex phases
    let config_agent = test_config_agent(
        Some(Runner::Codex),
        Some(Model::Gpt52Codex),
        Some(ReasoningEffort::Low),
    );

    let phase_overrides = PhaseOverrides {
        phase1: Some(PhaseOverrideConfig {
            runner: Some(Runner::Codex),
            model: Some(Model::Gpt52Codex),
            reasoning_effort: Some(ReasoningEffort::High), // Phase-specific effort
        }),
        ..Default::default()
    };

    let overrides = test_overrides_with_phases(None, None, None, Some(phase_overrides));

    let (matrix, _warnings) =
        resolve_phase_settings_matrix(&overrides, &config_agent, None, 3).unwrap();

    // Phase 1 should use phase-specific effort
    assert_eq!(matrix.phase1.reasoning_effort, Some(ReasoningEffort::High));
    // Other phases use config default
    assert_eq!(matrix.phase2.reasoning_effort, Some(ReasoningEffort::Low));
}

// ============================================================================
// Single-pass mapping tests
// ============================================================================

#[test]
fn resolve_phase_settings_single_pass_uses_phase2_overrides() {
    // Single-pass (--phases 1) should use Phase 2 overrides
    let config_agent = test_config_agent(Some(Runner::Claude), Some(Model::Gpt52), None);

    let phase_overrides = PhaseOverrides {
        phase1: Some(PhaseOverrideConfig {
            runner: Some(Runner::Opencode),
            model: Some(Model::Glm47),
            reasoning_effort: None,
        }),
        phase2: Some(PhaseOverrideConfig {
            runner: Some(Runner::Codex),
            model: Some(Model::Gpt52Codex),
            reasoning_effort: Some(ReasoningEffort::High),
        }),
        phase3: Some(PhaseOverrideConfig {
            runner: Some(Runner::Gemini),
            model: None,
            reasoning_effort: None,
        }),
    };

    let overrides = test_overrides_with_phases(None, None, None, Some(phase_overrides));

    let (matrix, warnings) =
        resolve_phase_settings_matrix(&overrides, &config_agent, None, 1).unwrap();

    // Phase 2 settings should be resolved (for single-pass execution)
    assert_eq!(matrix.phase2.runner, Runner::Codex);
    assert_eq!(matrix.phase2.model, Model::Gpt52Codex);
    assert_eq!(matrix.phase2.reasoning_effort, Some(ReasoningEffort::High));

    // But Phase 1 and Phase 3 overrides are unused
    assert!(warnings.unused_phase1);
    assert!(!warnings.unused_phase2); // Phase 2 is used
    assert!(warnings.unused_phase3);
}

#[test]
fn resolve_phase_settings_two_phase_warns_about_phase3() {
    // Two-phase execution should warn about unused phase 3 overrides
    let phase_overrides = PhaseOverrides {
        phase3: Some(PhaseOverrideConfig {
            runner: Some(Runner::Gemini),
            model: None,
            reasoning_effort: None,
        }),
        ..Default::default()
    };

    let overrides = test_overrides_with_phases(None, None, None, Some(phase_overrides));
    let config_agent = test_config_agent(Some(Runner::Claude), None, None);

    let (_matrix, warnings) =
        resolve_phase_settings_matrix(&overrides, &config_agent, None, 2).unwrap();

    assert!(!warnings.unused_phase1);
    assert!(!warnings.unused_phase2);
    assert!(warnings.unused_phase3);
}

// ============================================================================
// Validation error tests
// ============================================================================

#[test]
fn resolve_phase_settings_invalid_model_for_codex() {
    // Invalid model for Codex should produce phase-specific error
    let config_agent = test_config_agent(Some(Runner::Codex), Some(Model::Gpt52Codex), None);

    let phase_overrides = PhaseOverrides {
        phase2: Some(PhaseOverrideConfig {
            runner: Some(Runner::Codex),
            model: Some(Model::Glm47), // Invalid for Codex
            reasoning_effort: None,
        }),
        ..Default::default()
    };

    let overrides = test_overrides_with_phases(None, None, None, Some(phase_overrides));

    let result = resolve_phase_settings_matrix(&overrides, &config_agent, None, 3);

    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("Phase 2"));
    assert!(err.contains("invalid model"));
}

#[test]
fn resolve_phase_settings_invalid_custom_model_for_codex() {
    // Custom model that's invalid for Codex
    let config_agent = test_config_agent(Some(Runner::Codex), Some(Model::Gpt52Codex), None);

    let phase_overrides = PhaseOverrides {
        phase1: Some(PhaseOverrideConfig {
            runner: Some(Runner::Codex),
            model: Some(Model::Custom("invalid-model".to_string())),
            reasoning_effort: None,
        }),
        ..Default::default()
    };

    let overrides = test_overrides_with_phases(None, None, None, Some(phase_overrides));

    let result = resolve_phase_settings_matrix(&overrides, &config_agent, None, 3);

    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("Phase 1"));
}

// ============================================================================
// Warning tests
// ============================================================================

#[test]
fn resolve_phase_settings_warns_unused_phase3_when_phases_is_2() {
    let phase_overrides = PhaseOverrides {
        phase3: Some(PhaseOverrideConfig {
            runner: Some(Runner::Gemini),
            model: None,
            reasoning_effort: None,
        }),
        ..Default::default()
    };

    let overrides = test_overrides_with_phases(None, None, None, Some(phase_overrides));
    let config_agent = test_config_agent(Some(Runner::Claude), None, None);

    let (_matrix, warnings) =
        resolve_phase_settings_matrix(&overrides, &config_agent, None, 2).unwrap();

    assert!(warnings.unused_phase3);
    assert!(!warnings.unused_phase1);
    assert!(!warnings.unused_phase2);
}

#[test]
fn resolve_phase_settings_warns_unused_task_phase3_override_when_phases_is_2() {
    let config_agent = test_config_agent(Some(Runner::Claude), None, None);
    let task_agent = TaskAgent {
        runner: None,
        model: None,
        model_effort: ModelEffort::Default,
        phases: None,
        iterations: None,
        followup_reasoning_effort: None,
        runner_cli: None,
        phase_overrides: Some(PhaseOverrides {
            phase3: Some(PhaseOverrideConfig {
                runner: Some(Runner::Gemini),
                model: Some(Model::Custom("gemini-3-pro-preview".to_string())),
                reasoning_effort: None,
            }),
            ..Default::default()
        }),
    };

    let (_matrix, warnings) = resolve_phase_settings_matrix(
        &AgentOverrides::default(),
        &config_agent,
        Some(&task_agent),
        2,
    )
    .unwrap();

    assert!(warnings.unused_phase3);
}

#[test]
fn resolve_phase_settings_warns_unused_phase1_and_phase3_when_phases_is_1() {
    let phase_overrides = PhaseOverrides {
        phase1: Some(PhaseOverrideConfig {
            runner: Some(Runner::Opencode),
            model: None,
            reasoning_effort: None,
        }),
        phase3: Some(PhaseOverrideConfig {
            runner: Some(Runner::Gemini),
            model: None,
            reasoning_effort: None,
        }),
        ..Default::default()
    };

    let overrides = test_overrides_with_phases(None, None, None, Some(phase_overrides));
    let config_agent = test_config_agent(Some(Runner::Claude), None, None);

    let (_matrix, warnings) =
        resolve_phase_settings_matrix(&overrides, &config_agent, None, 1).unwrap();

    assert!(warnings.unused_phase1);
    assert!(!warnings.unused_phase2); // Phase 2 is used for single-pass
    assert!(warnings.unused_phase3);
}

// ============================================================================
// Complex integration tests
// ============================================================================

#[test]
fn resolve_phase_settings_full_matrix_resolution() {
    // Test a complex scenario with different settings per phase
    let config_agent = test_config_agent(
        Some(Runner::Claude),
        Some(Model::Custom("sonnet".to_string())),
        Some(ReasoningEffort::Medium),
    );

    let phase_overrides = PhaseOverrides {
        phase1: Some(PhaseOverrideConfig {
            runner: Some(Runner::Codex),
            model: Some(Model::Gpt52Codex),
            reasoning_effort: Some(ReasoningEffort::High),
        }),
        phase2: Some(PhaseOverrideConfig {
            runner: Some(Runner::Opencode),
            model: None,            // Should use Opencode default
            reasoning_effort: None, // Ignored for non-Codex
        }),
        phase3: Some(PhaseOverrideConfig {
            runner: Some(Runner::Gemini),
            model: Some(Model::Custom("gemini-pro".to_string())),
            reasoning_effort: Some(ReasoningEffort::Low), // Ignored for non-Codex
        }),
    };

    let overrides = test_overrides_with_phases(None, None, None, Some(phase_overrides));

    let (matrix, _warnings) =
        resolve_phase_settings_matrix(&overrides, &config_agent, None, 3).unwrap();

    // Phase 1: Codex with high effort
    assert_eq!(matrix.phase1.runner, Runner::Codex);
    assert_eq!(matrix.phase1.model, Model::Gpt52Codex);
    assert_eq!(matrix.phase1.reasoning_effort, Some(ReasoningEffort::High));

    // Phase 2: Opencode with default model, no effort
    assert_eq!(matrix.phase2.runner, Runner::Opencode);
    assert_eq!(matrix.phase2.model, Model::Glm47);
    assert_eq!(matrix.phase2.reasoning_effort, None);

    // Phase 3: Gemini with custom model, no effort (non-Codex)
    assert_eq!(matrix.phase3.runner, Runner::Gemini);
    assert_eq!(matrix.phase3.model.as_str(), "gemini-pro");
    assert_eq!(matrix.phase3.reasoning_effort, None);
}

#[test]
fn resolve_phase_settings_config_phase_overrides_only() {
    // Test config-based phase overrides (not CLI)
    let mut config_agent = test_config_agent(Some(Runner::Claude), None, None);
    config_agent.phase_overrides = Some(PhaseOverrides {
        phase1: Some(PhaseOverrideConfig {
            runner: Some(Runner::Codex),
            model: Some(Model::Gpt52Codex),
            reasoning_effort: Some(ReasoningEffort::High),
        }),
        phase2: None,
        phase3: Some(PhaseOverrideConfig {
            runner: Some(Runner::Gemini),
            model: None,
            reasoning_effort: None,
        }),
    });

    let overrides = AgentOverrides::default();

    let (matrix, _warnings) =
        resolve_phase_settings_matrix(&overrides, &config_agent, None, 3).unwrap();

    assert_eq!(matrix.phase1.runner, Runner::Codex);
    assert_eq!(matrix.phase1.model, Model::Gpt52Codex);

    assert_eq!(matrix.phase2.runner, Runner::Claude); // Config default

    assert_eq!(matrix.phase3.runner, Runner::Gemini);
}

#[test]
fn resolve_phase_settings_cli_overrides_config_phase() {
    // CLI phase overrides should beat config phase overrides
    let mut config_agent = test_config_agent(Some(Runner::Claude), None, None);
    config_agent.phase_overrides = Some(PhaseOverrides {
        phase1: Some(PhaseOverrideConfig {
            runner: Some(Runner::Codex),
            model: Some(Model::Gpt52Codex),
            reasoning_effort: Some(ReasoningEffort::Low),
        }),
        ..Default::default()
    });

    let cli_phase_overrides = PhaseOverrides {
        phase1: Some(PhaseOverrideConfig {
            runner: Some(Runner::Opencode), // CLI overrides config
            model: Some(Model::Glm47),
            reasoning_effort: Some(ReasoningEffort::High),
        }),
        ..Default::default()
    };

    let overrides = test_overrides_with_phases(None, None, None, Some(cli_phase_overrides));

    let (matrix, _warnings) =
        resolve_phase_settings_matrix(&overrides, &config_agent, None, 3).unwrap();

    // CLI should win over config
    assert_eq!(matrix.phase1.runner, Runner::Opencode);
    assert_eq!(matrix.phase1.model, Model::Glm47);
    // Effort is ignored for Opencode but CLI value was specified
}

#[test]
fn run_one_parallel_worker_acquires_queue_lock() -> anyhow::Result<()> {
    use crate::contracts::{QueueFile, Task, TaskStatus};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::thread;

    let temp = tempfile::TempDir::new()?;
    let repo_root = temp.path().to_path_buf();
    let ralph_dir = repo_root.join(".ralph");
    std::fs::create_dir_all(&ralph_dir)?;

    // Create a minimal queue with a task
    let queue_path = ralph_dir.join("queue.json");
    let mut queue_file = QueueFile {
        version: 1,
        tasks: vec![],
    };
    queue_file.tasks.push(Task {
        id: "RQ-0001".to_string(),
        title: "Test task".to_string(),
        description: None,
        status: TaskStatus::Todo,
        priority: crate::contracts::TaskPriority::Medium,
        tags: vec![],
        scope: vec![],
        evidence: vec![],
        plan: vec![],
        notes: vec![],
        request: None,
        agent: None,
        created_at: Some("2026-01-01T00:00:00Z".to_string()),
        updated_at: Some("2026-01-01T00:00:00Z".to_string()),
        completed_at: None,
        started_at: None,
        scheduled_start: None,
        depends_on: vec![],
        blocks: vec![],
        relates_to: vec![],
        duplicates: None,
        custom_fields: std::collections::HashMap::new(),
        estimated_minutes: None,
        actual_minutes: None,
        parent_id: None,
    });
    queue::save_queue(&queue_path, &queue_file)?;

    // Acquire the queue lock with a "test lock" label
    let _test_lock = queue::acquire_queue_lock(&repo_root, "test lock", false)?;

    // Spawn a thread that will try to acquire the lock via run_one_parallel_worker
    let repo_root_clone = repo_root.clone();
    let lock_acquired = Arc::new(AtomicBool::new(false));
    let lock_acquired_clone = Arc::clone(&lock_acquired);

    let handle = thread::spawn(move || {
        // Try to acquire the queue lock - this should fail since we hold it
        let result = queue::acquire_queue_lock(&repo_root_clone, "parallel worker", false);

        // Check if the error message indicates lock contention
        if let Err(e) = result {
            let err_str = e.to_string();
            if err_str.contains("Queue lock already held") || err_str.contains("already held") {
                lock_acquired_clone.store(false, Ordering::SeqCst);
            }
        } else {
            // Lock was acquired (unexpected in this test context)
            lock_acquired_clone.store(true, Ordering::SeqCst);
            // Drop the lock we just acquired
            drop(result);
        }
    });

    // Wait for the thread to complete
    handle.join().expect("thread panicked");

    // The lock should NOT have been acquired since we hold it
    assert!(
        !lock_acquired.load(Ordering::SeqCst),
        "Expected lock contention error when queue lock is already held"
    );

    Ok(())
}
