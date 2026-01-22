## PHASE 2 HANDOFF CHECKLIST (3-PHASE WORKFLOW)
When Phase 2 implementation is complete, you MUST:
1. If the CI gate is enabled ({{config.agent.ci_gate_enabled}}), run `{{config.agent.ci_gate_command}}` and fix failures until it is green.
2. Do NOT run `ralph task done`, `git commit`, or `git push` in Phase 2.
3. Leave the working tree dirty with the task changes for Phase 3 review (do not revert/stash).
4. Summarize what changed and any remaining risks or follow-ups.
5. Stop after CI passes; Phase 3 will handle code review and completion.
