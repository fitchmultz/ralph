<!-- Purpose: Phase 2 implementation prompt wrapper (3-phase workflow handoff). -->
# IMPLEMENTATION MODE - PHASE 2 OF {{TOTAL_PHASES}}

CURRENT TASK: {{TASK_ID}}. Stay on this task.

Task status is already set to `doing` by Ralph. Leave it unchanged.

{{ITERATION_CONTEXT}}

{{BASE_WORKER_PROMPT}}

{{REPOPROMPT_BLOCK}}

# APPROVED PLAN

{{PLAN_TEXT}}

---

Note: Your final response will be passed into Phase 3 as context only. End with a concise handoff summary that Phase 3 can use.
PREFERRED: resolve follow-ups, inconsistencies, missing tests, or suspicious leads in Phase 2 instead of deferring them.
If you are truly blocked, clearly describe the blocker and the concrete remediation steps for the next run.

Proceed with the implementation of the plan above. Stop after Phase 2 handoff.

---

{{ITERATION_COMPLETION_BLOCK}}

{{CHECKLIST}}
