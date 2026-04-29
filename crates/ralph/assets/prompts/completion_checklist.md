<!-- Purpose: Shared final completion checklist injected into implementation-mode worker prompts. -->
# IMPLEMENTATION COMPLETION CHECKLIST
Run mode: `{{RUN_MODE}}` (`normal` or `parallel-worker`).

Complete these in order.

1. Outcome check
   - Verify the active task scope is complete.
   - Resolve in-scope risks, bugs, missing tests, or suspicious leads before completion when practical.
   - If a lead is false, note why in the final response.

2. Follow-up proposals
   - follow-ups cannot substitute for finishing the active task's current scope.
   - Create `.ralph/cache/followups/{{TASK_ID}}.json` only for independent out-of-scope work or explicit discovery/queue-shaping deliverables.
   - `RUN_MODE=normal`: apply proposals with `ralph task followups apply --task {{TASK_ID}}` before terminal bookkeeping.
   - `RUN_MODE=parallel-worker`: do not apply the proposal; leave the artifact for coordinator integration.
   - If there is no independent follow-up work, skip the artifact.

3. Validation
   - CI Gate (configured validation only; never a run toggle): `agent.ci_gate.enabled=false` skips Ralph-managed CI validation only. It does NOT disable this run, task execution, queue bookkeeping, or git publish behavior.
   - If no files changed, you may skip the configured CI gate and say why.
   - If files changed and `agent.ci_gate.enabled` is true (`{{config.agent.ci_gate_enabled}}`), run `{{config.agent.ci_gate_display}}` and fix failures before ending.
   - If files changed and `agent.ci_gate.enabled=false`, do not invent that CI requirement; state that configured CI validation was skipped because `agent.ci_gate.enabled=false`, and list other checks run.
   - Ensure `{{config.queue.file}}` remains valid if queue state changed.

4. Task bookkeeping
   - `RUN_MODE=normal`: finish with `ralph task done --note "<note>" {{TASK_ID}}` or `ralph task reject --note "<note>" {{TASK_ID}}`.
   - `RUN_MODE=parallel-worker`: do not run `ralph task done` or manually rewrite queue/done; Ralph reconciles bookkeeping after integration.
   - Do not run `ralph queue archive` for single-task completion.
   - If stopping incomplete, leave the task active and clearly report state and next step; do not set `blocked`.

5. Git hygiene
   - If `{{config.agent.git_publish_mode}}` is `commit_and_push`, do not manually commit or push; Ralph handles publish.
   - In `parallel-worker` mode, leave the workspace rebased, conflict-free, committed, and CI-clean when enabled; Ralph validates bookkeeping and pushes.

# Final Response Shape
- Summary of completed outcome
- Validation run and result
- Task/follow-up bookkeeping performed
- Remaining risks or blockers, if any
