<!-- Purpose: Phase 2 implementation prompt wrapper (2-phase workflow). -->
# IMPLEMENTATION MODE - PHASE 2 OF {{TOTAL_PHASES}}
Task: `{{TASK_ID}}`

# Goal
Implement the approved plan, adapt it to repo reality where needed, validate the result, and complete final task bookkeeping through the checklist.

Task status is already `doing`; leave it unchanged until checklist instructions say otherwise.

{{ITERATION_CONTEXT}}

{{BASE_WORKER_PROMPT}}

{{REPOPROMPT_BLOCK}}

# Approved Plan
{{PLAN_TEXT}}

# Execution Rules
- Use the approved plan as the starting contract.
- If repo reality conflicts with the plan, preserve the task goal and choose the smallest safe adaptation.
- Update all downstream docs/config/tests/user surfaces affected by the change.
- Prefer finishing current scope over deferring work.

{{ITERATION_COMPLETION_BLOCK}}

{{CHECKLIST}}
