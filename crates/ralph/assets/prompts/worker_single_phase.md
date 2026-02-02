<!-- Purpose: Single-phase execution prompt wrapper (plan+implement). -->
{{REPOPROMPT_BLOCK}}

CURRENT TASK: {{TASK_ID}}. Do NOT switch tasks.

{{ITERATION_CONTEXT}}

{{ITERATION_COMPLETION_BLOCK}}

{{CHECKLIST}}

You are in single-pass execution mode. You may do brief planning, but you are NOT required to produce a separate plan first. Proceed directly to implementation.

---

{{BASE_WORKER_PROMPT}}
