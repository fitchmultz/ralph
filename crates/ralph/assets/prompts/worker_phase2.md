<!-- Purpose: Phase 2 implementation prompt wrapper (2-phase workflow). -->
# IMPLEMENTATION MODE - PHASE 2 OF {{TOTAL_PHASES}}
CURRENT TASK: {{TASK_ID}}. Do NOT switch tasks.
Task status is already `doing` (do not change it).

## PHASE EXECUTION STYLE
Use swarms and sub-agents aggressively:
- Parallelize implementation, regression search, and validation where streams are independent.
- Use sub-agents to handle bounded modules/tests, then merge and verify outcomes.

{{ITERATION_CONTEXT}}

{{BASE_WORKER_PROMPT}}

{{REPOPROMPT_BLOCK}}

# APPROVED PLAN
{{PLAN_TEXT}}

## EXECUTION CONTRACT
- Implement the approved plan end-to-end.
- If plan details conflict with repository reality, adapt safely and record deviations.
- Fix root cause and related occurrences inside the relevant blast radius.
- Add/update tests for both expected behavior and failure modes.
- Keep implementation minimal, cohesive, and maintainable.

## RESPONSE CONTRACT
End with:
1. Files changed and behavior impact
2. Validation commands run and results
3. Plan deviations (if any) and rationale
4. Remaining risks or suspicious leads

{{ITERATION_COMPLETION_BLOCK}}

{{CHECKLIST}}
