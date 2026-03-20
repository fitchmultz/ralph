## PHASE 2 HANDOFF CHECKLIST (3-PHASE WORKFLOW)
Follow this checklist. REQUIRED items are workflow-critical.

1. REQUIRED: if the CI gate is enabled ({{config.agent.ci_gate_enabled}}), run `{{config.agent.ci_gate_display}}` and fix failures until it is green.
2. REQUIRED: do not run `ralph task done`, `git commit`, or `git push` in Phase 2.
3. REQUIRED: leave the working tree dirty with the task changes for Phase 3 review (do not revert or stash).
4. PREFERRED: resolve follow-ups, inconsistencies, missing tests, or suspicious leads in Phase 2 instead of deferring them.
5. If you are truly blocked, clearly summarize the blocker and include exact remediation steps for the next run.
6. PREFERRED: summarize what changed and how to verify it with exact commands when practical.
7. REQUIRED: stop after CI passes; Phase 3 owns review, refinement, and terminal task bookkeeping.
