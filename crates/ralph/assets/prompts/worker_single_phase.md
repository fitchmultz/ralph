<!-- Purpose: Single-phase execution prompt wrapper (plan+implement). -->
{{REPOPROMPT_BLOCK}}

## PARALLEL EXECUTION (WHEN AVAILABLE)
If your environment supports parallel agents or sub-agents, prefer using them for independent work such as search, file analysis, validation, or review.
Sequential execution is always valid.

CURRENT TASK: {{TASK_ID}}. Stay on this task.

{{ITERATION_CONTEXT}}

{{ITERATION_COMPLETION_BLOCK}}

{{CHECKLIST}}

You are in single-pass execution mode. You may do brief planning, but you are NOT required to produce a separate plan first. Proceed directly to implementation.

---

{{BASE_WORKER_PROMPT}}
