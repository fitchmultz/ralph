//! Shared prompt blocks for multi-iteration refinement behavior.

pub const ITERATION_CONTEXT_REFINEMENT: &str = r#"
## REFINEMENT CONTEXT
A prior execution of this task already occurred in this run. Focus on refinement:
- identify regressions or unintended behavior changes
- simplify or harden the implementation where possible
- do NOT assume the task is complete
- check for an existing plan and reuse the same file path if it exists. do not create a plan document outside of .ralph/cache/plans.

The working tree may already be dirty from earlier work. Do NOT stop because the repo is dirty.
"#;

pub const ITERATION_COMPLETION_BLOCK: &str = r#"
## ITERATION COMPLETION RULES
This run must NOT complete the task.
- Do NOT run `ralph task done` or `ralph task reject`.
- Leave the task status as `doing`.
- Leave the working tree dirty for continued iteration.
"#;

pub const PHASE3_COMPLETION_GUIDANCE_FINAL: &str = "Task status is already set to `doing` by Ralph. Do NOT change it (use `ralph task done` when finished). Investigate and resolve any risks, bugs, or suspicious leads you flag before completion; Do NOT complete the task if any lead remains unresolved. If a lead is a false positive, document why in your final response.";

pub const PHASE3_COMPLETION_GUIDANCE_NONFINAL: &str = "Task status is already set to `doing` by Ralph. Do NOT change it. Do NOT run `ralph task done` or `ralph task reject` in this run. Investigate and resolve any risks, bugs, or suspicious leads you flag before ending the run; Do NOT end the run if any lead remains unresolved. If a lead is a false positive, document why in your summary.";
