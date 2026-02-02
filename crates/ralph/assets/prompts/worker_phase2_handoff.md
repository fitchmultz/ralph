<!-- Purpose: Phase 2 implementation prompt wrapper (3-phase workflow handoff). -->
# IMPLEMENTATION MODE - PHASE 2 OF {{TOTAL_PHASES}}

CURRENT TASK: {{TASK_ID}}. Do NOT switch tasks.

Task status is already set to `doing` by Ralph. Do NOT change it.

{{ITERATION_CONTEXT}}

{{BASE_WORKER_PROMPT}}

{{REPOPROMPT_BLOCK}}

# APPROVED PLAN

{{PLAN_TEXT}}

---

Note: Your final response will be passed into Phase 3 as context only. End with a clear, concise final response that Phase 3 can use.
If you identify unresolved risks, bugs, or suspicious leads, list them explicitly in your final response so Phase 3 can close them.

Proceed with the implementation of the plan above. Stop after Phase 2 handoff.

---

{{ITERATION_COMPLETION_BLOCK}}

{{CHECKLIST}}
