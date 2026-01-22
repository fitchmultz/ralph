<!-- Purpose: Shared completion checklist injected into implementation-mode worker prompts. -->
## IMPLEMENTATION COMPLETION CHECKLIST
When implementation is complete, you MUST:
1. Run `ralph task done --note "<note>" {{TASK_ID}}` to move the task from `.ralph/queue.json` to `.ralph/done.json`.
   - Use `ralph task reject --note "<note>" {{TASK_ID}}` when appropriate; only `done` and `rejected` are valid completion statuses.
   - Provide 1-5 summary notes using repeated `--note` flags (each note should be a short bullet).
2. If the task is incomplete but you are stopping:
   - Leave it in `.ralph/queue.json` as `doing` (or revert to `todo` if not continuing).
   - Do NOT set `blocked`.
3. Do NOT manually edit `.ralph/queue.json` or `.ralph/done.json` to complete tasks, and do not run `ralph queue archive` for single-task completion.
4. Ensure `.ralph/queue.json` remains valid JSON and respects the queue contract.
5. If the CI gate is enabled ({{config.agent.ci_gate_enabled}}), run `{{config.agent.ci_gate_command}}` and fix all failures before ending your turn.
6. Git hygiene:
   - Do NOT commit or push until `ralph task done` succeeds.
   - Commit ALL changes (including `.ralph/queue.json`) with `RQ-####: <short summary>`.
   - Push your commit(s) so the branch is not ahead of upstream.
   - Confirm the repo is clean: `git status --porcelain` is empty.
   - If you cannot push (no upstream/permissions), stop and report the blocker.
