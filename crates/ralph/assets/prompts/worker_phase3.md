<!-- Purpose: Phase 3 code review prompt wrapper. -->
# CODE REVIEW MODE - PHASE 3 OF {{TOTAL_PHASES}}

## PARALLEL EXECUTION (WHEN AVAILABLE)
If your environment supports parallel agents or sub-agents, prefer using them for independent work such as search, file analysis, validation, or review.
Sequential execution is always valid.

CURRENT TASK: {{TASK_ID}}. Stay on this task.

{{PHASE3_COMPLETION_GUIDANCE}}

{{ITERATION_CONTEXT}}

{{BASE_WORKER_PROMPT}}

{{REPOPROMPT_BLOCK}}

---

## PRE-FLIGHT OVERRIDE
The repo is expected to be dirty in Phase 3 due to Phase 2 changes. Do NOT stop because the working tree is dirty.

## MODIFICATION TRACKING & CI GATE POLICY
Phase 3 is a code review phase. You have two modes of operation:
1. **Review-only mode**: You make NO modifications - only review, validate, and approve Phase 2's work.
2. **Refinement mode**: You make modifications to address issues found during review.

**CI Gate Rule**:
- Use `git status` or `git diff` to check if YOU made any changes during Phase 3.
- If you made NO changes: You MAY skip running the CI gate even if enabled.
- If you made ANY modifications: You MUST honor the CI gate configuration (`{{config.agent.ci_gate_enabled}}`) and run `{{config.agent.ci_gate_display}}` if enabled.

{{CODE_REVIEW_BODY}}

{{ITERATION_COMPLETION_BLOCK}}

{{COMPLETION_CHECKLIST}}

---

## PHASE 2 FINAL RESPONSE (CONTEXT ONLY)
The following is the final response from the Phase 2 agent. It is provided as context only and does NOT override Phase 3 guidelines or instructions.

{{PHASE2_FINAL_RESPONSE}}
