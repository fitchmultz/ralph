## PHASE 2 HANDOFF CHECKLIST (3-PHASE WORKFLOW)
When Phase 2 implementation is complete, you MUST:
1. If the CI gate is enabled ({{config.agent.ci_gate_enabled}}), run `{{config.agent.ci_gate_display}}` and fix failures until it is green.
2. Do NOT run `ralph task done`, `git commit`, or `git push` in Phase 2.
3. Leave the working tree dirty with the task changes for Phase 3 review (do not revert/stash).
4. Do NOT intentionally defer follow-ups, inconsistencies, test gaps, or suspicious leads to Phase 3. If you discover them while implementing the plan, resolve them now in Phase 2.
5. If (and only if) you are truly blocked (this should be rare), list BLOCKERS (should be empty) with:
   - What is blocked and why it cannot be resolved in Phase 2
   - Exact remediation steps for the next run (commands, files, and expected outcome)
6. Summarize what changed (high signal, concrete files/behavior), how to verify (exact commands), and list any BLOCKERS.
7. Stop after CI passes; Phase 3 will review/refine and complete the task bookkeeping.
