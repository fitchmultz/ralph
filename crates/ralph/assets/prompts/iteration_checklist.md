<!-- Purpose: Checklist for non-final iterations (refinement mode). -->
## ITERATION CHECKLIST (REFINEMENT MODE)
Follow this checklist. REQUIRED items protect workflow invariants; everything else is outcome guidance.

1. PREFERRED: verify behavior against task requirements and look for regressions or unintended side effects.
2. PREFERRED: investigate and resolve suspicious leads discovered during the iteration, or explain why they are false positives.
3. PREFERRED: simplify or deduplicate code where it improves clarity while keeping behavior correct.
4. PREFERRED: tighten tests for changed behavior and newly discovered failure modes.
5. CI Gate:
   - if you made no changes, you may skip the CI gate
   - REQUIRED: if you made changes and the CI gate is enabled, run `{{config.agent.ci_gate_display}}` and fix failures until it is green
6. PREFERRED: summarize changes, remaining risks, and follow-up work for the next run.
7. REQUIRED: do not run `ralph task done`; leave the task active for continued iteration.
