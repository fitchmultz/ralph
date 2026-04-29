//! Shared prompt blocks for multi-iteration refinement behavior.
//!
//! Purpose:
//! - Shared prompt blocks for multi-iteration refinement behavior.
//!
//! Responsibilities: define static prompt sections reused across multi-iteration flows.
//!
//! Scope:
//! - Limited to this file's owning feature boundary.
//!
//! Not handled: prompt loading, template rendering, or queue/task mutations.
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions: text blocks are consumed verbatim and remain aligned with phase rules.

pub(crate) const ITERATION_CONTEXT_REFINEMENT: &str = r#"
## REFINEMENT CONTEXT
A prior pass already worked on this task. Use that state as evidence, not proof of completion.

Success for this pass means:
- regressions or unintended behavior changes are found and addressed
- touched code is simplified or hardened where practical
- any existing plan cache path is reused instead of creating a competing plan artifact
- remaining risks are reported clearly for the next pass

The working tree may already be dirty from earlier work. Inspect it, then continue; do not stop for expected task changes alone.
"#;

pub(crate) const ITERATION_COMPLETION_BLOCK: &str = r#"
## ITERATION COMPLETION RULES
This is not the terminal completion run.
- Do not run `ralph task done` or `ralph task reject`.
- Leave the task status as `doing`.
- Leave task work available for continued iteration; do not stash or revert completed in-scope work.
"#;

pub(crate) const PHASE3_COMPLETION_GUIDANCE_FINAL: &str = "Task status is already `doing`. Leave it unchanged until the completion checklist performs terminal bookkeeping. Before completion, resolve in-scope risks, bugs, missing tests, or suspicious leads when practical; if a lead is false, state the evidence briefly.";

pub(crate) const PHASE3_COMPLETION_GUIDANCE_NONFINAL: &str = "Task status is already `doing`. Leave it unchanged. This is not a terminal run: do not run `ralph task done` or `ralph task reject`. Investigate in-scope risks and suspicious leads when practical, and summarize evidence plus next steps for the next pass.";
