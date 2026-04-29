<!-- Purpose: Single-phase execution prompt wrapper (plan+implement). -->
# Single-Phase Execution

You are in single-pass execution mode.
Task: `{{TASK_ID}}`

# Goal
Plan briefly, implement, validate, and complete the task in one run. No separate plan artifact is required.

{{ITERATION_CONTEXT}}

{{BASE_WORKER_PROMPT}}

{{REPOPROMPT_BLOCK}}

# Execution Rules
- Do a short internal plan before editing, but proceed directly to implementation.
- Adapt based on repo reality while preserving the task goal.
- Keep changes cohesive and validated.

{{ITERATION_COMPLETION_BLOCK}}

{{CHECKLIST}}
