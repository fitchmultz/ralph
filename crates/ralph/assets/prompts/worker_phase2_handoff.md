<!-- Purpose: Phase 2 implementation prompt wrapper (3-phase workflow handoff). -->
# IMPLEMENTATION MODE - PHASE 2 OF {{TOTAL_PHASES}}
CURRENT TASK: {{TASK_ID}}. Do NOT switch tasks.
Task status is already `doing` (do not change it).

## PHASE EXECUTION STYLE
Use swarms and sub-agents aggressively:
- Parallelize independent implementation/validation streams.
- Delegate bounded work to sub-agents, then synthesize and verify centrally.

{{ITERATION_CONTEXT}}

{{BASE_WORKER_PROMPT}}

{{REPOPROMPT_BLOCK}}

# APPROVED PLAN
{{PLAN_TEXT}}

## EXECUTION CONTRACT
- Implement the approved plan end-to-end.
- If plan and codebase differ, adapt to reality and document deviations.
- Fix root cause and related occurrences within blast radius.
- Add/update tests for success and failure behavior.

## PHASE 2 HANDOFF CONTRACT
Your final response is context for Phase 3. Keep it concise and structured:
1. Files changed + behavior impact
2. Validation commands + results
3. Deviations from plan (if any)
4. Remaining risks, bugs, or suspicious leads that Phase 3 must close

If you identify unresolved risks, bugs, or suspicious leads, list them explicitly in your final response so Phase 3 can close them.

Stop after Phase 2 handoff.

{{ITERATION_COMPLETION_BLOCK}}

{{CHECKLIST}}
