<!-- Purpose: Phase 1 planning prompt wrapper. -->
# PLANNING MODE - PHASE 1 OF {{TOTAL_PHASES}}

CURRENT TASK: {{TASK_ID}}. Do NOT switch tasks.

{{ITERATION_CONTEXT}}

{{BASE_WORKER_PROMPT}}

{{REPOPROMPT_BLOCK}}

## OUTPUT REQUIREMENT: PLAN ONLY
You are in Phase 1 (Planning). You must NOT implement the changes.
Your goal is to understand the task, gather context, and produce a detailed plan.

## STRICT PROHIBITIONS (PHASE 1 ONLY)
**DO NOT DO ANY OF THE FOLLOWING:**
- Create or modify any files, EXCEPT the plan cache file below (Ralph handles queue bookkeeping)
- Run tests, the configured CI gate command (`{{config.agent.ci_gate_command}}`) when enabled, or any validation commands
- Execute `git add`, `git commit`, or `git push`
- Write, edit, or change any source code, configuration, or documentation files
- Make any implementation changes whatsoever

**NO FILE EDITS ARE ALLOWED IN PHASE 1, EXCEPT writing the plan cache file below.**

**IF YOU START IMPLEMENTING:**
- STOP immediately
- Revert any file changes you made

## PLAN OUTPUT REQUIREMENT
You MUST write the final plan directly to this file:

{{PLAN_PATH}}

**Do NOT print the plan in your reply.** The plan must be written to the file above.
Use the available tooling to write the plan file directly.
After writing the file, you may respond with a brief confirmation only.

The plan should be detailed for Phase 2 implementation.
Treat any `context_builder` response as planning input only. Do NOT start implementing code after you receive it.
Do NOT switch tasks: plan ONLY for the current task and ignore any other IDs mentioned in tool output.
