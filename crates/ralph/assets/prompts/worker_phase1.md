<!-- Purpose: Phase 1 planning prompt wrapper. -->
# PLANNING MODE - PHASE 1 OF {{TOTAL_PHASES}}
Task: `{{TASK_ID}}`

# Goal
Produce a standalone implementation plan for the current task. Do not implement source/config/docs changes in this phase.

{{ITERATION_CONTEXT}}

{{BASE_WORKER_PROMPT}}

{{REPOPROMPT_BLOCK}}

# Task Refresh
{{TASK_REFRESH_INSTRUCTION}}

# Phase 1 Constraints
Allowed edits only:
- `{{config.queue.file}}` when the task refresh step requires it
- `{{PLAN_PATH}}` for the final plan

Do not edit source, config, or docs outside those files. Do not run implementation validation commands, the configured CI gate (`{{config.agent.ci_gate_display}}`), `git add`, `git commit`, or `git push`.

If implementation work starts accidentally, stop and revert disallowed edits.

# Plan Output Contract
Write the final plan directly to:

{{PLAN_PATH}}

The Phase 2 agent may only have this plan plus task context. Make it self-contained.

Required sections:
- Task goal and why it matters
- Current evidence and relevant files/commands inspected
- Proposed changes, including important files/symbols likely touched
- Acceptance criteria and validation commands
- Risks, assumptions, and fallback/adaptation guidance
- Compatibility or migration notes when behavior changes

Include focused code or command snippets when they materially reduce ambiguity. Do not paste whole files. State that Phase 2 may adapt the plan if repo reality differs, while preserving the task goal and safety constraints.

# Stop Rules
- Treat `context_builder` output as planning input only.
- Plan only for `{{TASK_ID}}`; ignore other task IDs mentioned in tool output unless they are dependencies/evidence for this task.
- Final response may be a brief confirmation that the plan was written; do not echo the full plan unless required by the harness.
