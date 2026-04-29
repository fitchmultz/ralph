<!-- Purpose: Phase 2 handoff checklist for 3-phase workflows. -->
# PHASE 2 HANDOFF CHECKLIST
Do not perform terminal task bookkeeping in Phase 2.

1. If files changed and `agent.ci_gate.enabled` is true (`{{config.agent.ci_gate_enabled}}`), run `{{config.agent.ci_gate_display}}` and fix failures until green.
2. If files changed and `agent.ci_gate.enabled=false`, only the configured CI command is skipped; Phase 2 implementation and handoff still continue. Report that configured CI validation was skipped by configuration.
3. Do not run `ralph task done`, `ralph task reject`, `git commit`, or `git push`.
4. Leave the working tree dirty with task changes for Phase 3 review; do not stash or revert completed work.
5. Resolve in-scope follow-ups, inconsistencies, missing tests, and suspicious leads when practical.
6. If independent follow-up work remains, write or mention `.ralph/cache/followups/{{TASK_ID}}.json` for Phase 3/coordinator handling.
7. If you are truly blocked, clearly summarize the blocker and exact remediation steps.
8. Stop after configured Phase 2 validation is satisfied. End with a concise handoff: changed files/outcome, validation result, follow-up proposal status, and blockers/risks.
