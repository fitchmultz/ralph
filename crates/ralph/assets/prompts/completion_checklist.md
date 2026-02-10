<!-- Purpose: Shared completion checklist injected into implementation-mode worker prompts. -->
## IMPLEMENTATION COMPLETION CHECKLIST
When implementation is complete:
1. Investigate and resolve any risks, bugs, or suspicious leads you flagged during this run. If a lead is false positive, document why.
2. CI Gate (conditional):
   - Check whether you made modifications this phase (`git status`/`git diff`).
   - If no modifications: CI rerun is optional.
   - If modifications: run `{{config.agent.ci_gate_command}}` when enabled (`{{config.agent.ci_gate_enabled}}`) and make it pass.
3. Complete the task lifecycle via Ralph CLI:
   - Done path: `ralph task done --note "<note>" {{TASK_ID}}`
   - Reject path: `ralph task reject --note "<note>" {{TASK_ID}}`
   - Only `done` and `rejected` are valid terminal statuses.
   - Use 1-5 concise `--note` entries.
4. Perform queue freshness check before done/rejected:
   - Scan other queue tasks for stale assumptions/plans/evidence caused by your change.
   - Update stale fields with `ralph task field <KEY> <VALUE> <TASK_ID>` using minimal high-signal notes.
5. If stopping without completion:
   - Leave task as `doing` (or revert to `todo` if not continuing).
   - Do not set `blocked`.
6. Do not manually edit `.ralph/queue.json` or `.ralph/done.json` for completion.
   - Do not run `ralph queue archive` for single-task completion.
7. Ensure `.ralph/queue.json` stays valid and contract-compliant.
8. Git hygiene:
   - Do not run `git commit`/`git push` before `ralph task done` succeeds.
   - If auto commit/push is enabled (`{{config.agent.git_commit_push_enabled}}`), Ralph performs commit/push.
   - If disabled, leave repo dirty and report manual commit/push required.
   - Confirm repo state: when auto commit/push is enabled, `git status --porcelain` should be empty after completion; otherwise report remaining changes.
   - If push is required but blocked (permissions/upstream), stop and report blocker.
