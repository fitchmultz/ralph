<!-- Purpose: Checklist for non-final iterations (refinement mode). -->
## ITERATION CHECKLIST (REFINEMENT MODE)
When refining an already-implemented task, you MUST:
1. Verify behavior against the task requirements and look for regressions or unintended side effects.
2. Investigate and resolve any risks, bugs, or suspicious leads you identify in this iteration before ending your run. If a lead is a false positive, document why in your summary; do not defer without explanation.
3. Simplify or deduplicate code where possible while keeping behavior correct.
4. Tighten tests to cover expected behavior and failure modes uncovered by the review.
5. CI Gate (Conditional):
   - Check if you made ANY modifications during this iteration using git status/diff.
   - If you made NO changes: you MAY skip the CI gate even if enabled.
   - If you made ANY modifications: you MUST run the CI gate if enabled (`{{config.agent.ci_gate_display}}`) and fix failures until it is green.
6. Summarize changes, remaining risks, and any follow-up work needed for the next run.
7. Do NOT run `ralph task done`, and leave the working tree dirty for continued iteration.
