//! Shared prompt blocks for multi-iteration refinement behavior.
//!
//! Responsibilities: define static prompt sections reused across multi-iteration flows.
//! Not handled: prompt loading, template rendering, or queue/task mutations.
//! Invariants/assumptions: text blocks are consumed verbatim and remain aligned with phase rules.

pub(crate) const ITERATION_CONTEXT_REFINEMENT: &str = r#"
## REFINEMENT CONTEXT
A prior execution of this task already occurred in this run. Focus on refinement:
- identify regressions or unintended behavior changes
- simplify or harden the implementation where possible
- do not assume the task is complete
- if a plan already exists, reuse the same file path instead of creating a new planning artifact elsewhere

The working tree may already be dirty from earlier work. Do not stop just because the repo is dirty.
"#;

pub(crate) const ITERATION_COMPLETION_BLOCK: &str = r#"
## ITERATION COMPLETION RULES
This run must not complete the task.
- REQUIRED: do not run `ralph task done` or `ralph task reject`.
- REQUIRED: leave the task status as `doing`.
- REQUIRED: leave the working tree dirty for continued iteration.
"#;

pub(crate) const PHASE3_COMPLETION_GUIDANCE_FINAL: &str = "Task status is already set to `doing` by Ralph. Leave it unchanged until terminal task bookkeeping is complete. PREFERRED: investigate and resolve any risks, bugs, or suspicious leads you flag before completion. If a lead is a false positive, document why in your final response.";

pub(crate) const PHASE3_COMPLETION_GUIDANCE_NONFINAL: &str = "Task status is already set to `doing` by Ralph. Leave it unchanged. REQUIRED: do not run `ralph task done` or `ralph task reject` in this run. PREFERRED: investigate and resolve any risks, bugs, or suspicious leads you flag before ending the run. If a lead is a false positive, document why in your summary.";
