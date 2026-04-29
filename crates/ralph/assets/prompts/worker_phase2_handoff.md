<!-- Purpose: Phase 2 implementation prompt wrapper (3-phase workflow handoff). -->
# IMPLEMENTATION MODE - PHASE 2 OF {{TOTAL_PHASES}}
Task: `{{TASK_ID}}`

# Goal
Implement the approved plan and leave a concise, useful handoff for Phase 3 review. Do not perform terminal task bookkeeping.

Task status is already `doing`; leave it unchanged.

{{ITERATION_CONTEXT}}

{{BASE_WORKER_PROMPT}}

{{REPOPROMPT_BLOCK}}

# Approved Plan
{{PLAN_TEXT}}

# Execution Rules
- Implement the plan, adapting only where repo reality requires it.
- Resolve follow-ups, inconsistencies, missing tests, and suspicious leads in Phase 2 when they are in scope.
- Leave the working tree ready for Phase 3 review.
- If independent follow-up work remains, write or mention `.ralph/cache/followups/{{TASK_ID}}.json` for Phase 3/coordinator handling.

# Handoff Output Contract
End with a concise handoff summary:
- changed files and user-visible behavior
- validation run and result, or why it could not run
- follow-up proposal status
- unresolved risks/blockers and concrete remediation steps if blocked

{{ITERATION_COMPLETION_BLOCK}}

{{CHECKLIST}}
