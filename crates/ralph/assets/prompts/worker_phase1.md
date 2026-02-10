<!-- Purpose: Phase 1 planning prompt wrapper. -->
# PLANNING MODE - PHASE 1 OF {{TOTAL_PHASES}}
CURRENT TASK: {{TASK_ID}}. Do NOT switch tasks.

## PHASE EXECUTION STYLE
Use swarms and sub-agents aggressively for planning:
- Parallelize context gathering across files/areas.
- Use sub-agents to challenge assumptions and close plan gaps.
- Reconcile findings into one coherent plan.

{{ITERATION_CONTEXT}}

{{BASE_WORKER_PROMPT}}

{{REPOPROMPT_BLOCK}}

## OBJECTIVE
Produce a standalone implementation plan for Phase 2.

## OUTPUT REQUIREMENT: PLAN ONLY
Plan only. Do not implement.

## PHASE 1 HARD CONSTRAINTS
- Allowed file writes: `{{PLAN_PATH}}` only.
- Forbidden: source/config/docs edits, tests, validation commands, `{{config.agent.ci_gate_command}}`, `git add`, `git commit`, `git push`.
- If you accidentally start implementing, stop immediately and revert all non-plan edits.
- NO FILE EDITS ARE ALLOWED IN PHASE 1, EXCEPT writing `{{PLAN_PATH}}`.

## PLAN QUALITY CONTRACT
Write the full plan to `{{PLAN_PATH}}`. Do not print the full plan in the reply.

Phase 2 has no additional context. The plan must be self-sufficient and include:
1. Task goal
2. Why it matters
3. Current-state analysis (key files/symbols/constraints)
4. Ordered implementation steps
5. Concrete code snippets for risky/non-obvious changes
6. Verification steps (commands + expected outcomes)
7. Risks, edge cases, and mitigations
8. Explicit instruction that Phase 2 may adapt if repo reality differs
9. No references to hidden context (avoid phrases like "as discussed"); Phase 2 only sees this plan
10. Backwards compatibility expectations (default: not required unless explicitly requested)

Treat any `context_builder` response as planning input only. Do not implement.
Do not switch tasks: plan only for the current task and ignore other task IDs mentioned by tools.
