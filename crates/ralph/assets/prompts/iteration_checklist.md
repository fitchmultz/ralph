<!-- Purpose: Checklist for non-final refinement iterations. -->
# ITERATION CHECKLIST
This is not the terminal completion run.

1. Verify current behavior against task requirements and look for regressions.
2. Fix or document suspicious leads discovered this iteration.
3. Simplify or deduplicate touched code when it improves correctness or clarity.
4. Add or tighten tests for changed behavior when practical.
5. Validation:
   - If no files changed, CI may be skipped with a note.
   - If files changed and `agent.ci_gate.enabled` is true (`{{config.agent.ci_gate_enabled}}`), run `{{config.agent.ci_gate_display}}` and fix failures.
   - If files changed and `agent.ci_gate.enabled=false`, skip only the configured CI command/requirement, continue the iteration, and report that configured CI validation was skipped by configuration.
6. Summarize changes, remaining risks, and next-step guidance for the next run.
7. Do not run `ralph task done` or `ralph task reject`; leave the task active for continued iteration.
