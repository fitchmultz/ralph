<!-- Purpose: Phase 2 implementation prompt wrapper (2-phase workflow). -->
# IMPLEMENTATION MODE - PHASE 2 OF {{TOTAL_PHASES}}

## PARALLEL EXECUTION (WHEN AVAILABLE)
If your environment supports parallel agents or sub-agents, prefer using them for independent work such as search, file analysis, validation, or review.
Sequential execution is always valid.

CURRENT TASK: {{TASK_ID}}. Stay on this task.

Task status is already set to `doing` by Ralph. Leave it unchanged.

{{ITERATION_CONTEXT}}

{{BASE_WORKER_PROMPT}}

{{REPOPROMPT_BLOCK}}

# APPROVED PLAN

{{PLAN_TEXT}}

---

Proceed with the implementation of the plan above.

---

{{ITERATION_COMPLETION_BLOCK}}

{{CHECKLIST}}
