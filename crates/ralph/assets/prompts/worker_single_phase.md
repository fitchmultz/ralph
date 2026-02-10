<!-- Purpose: Single-phase execution prompt wrapper (plan+implement). -->
{{REPOPROMPT_BLOCK}}

# SINGLE-PHASE EXECUTION MODE
Use swarms and sub-agents aggressively:
- Parallelize independent discovery, implementation, and validation streams.
- Delegate bounded sub-tasks to sub-agents, then reconcile and verify centrally.

CURRENT TASK: {{TASK_ID}}. Do NOT switch tasks.

{{ITERATION_CONTEXT}}

{{ITERATION_COMPLETION_BLOCK}}

{{CHECKLIST}}

You are in single-pass execution mode. You may do brief planning, but you are not required to produce a separate plan first. Proceed directly to implementation.

---

{{BASE_WORKER_PROMPT}}
