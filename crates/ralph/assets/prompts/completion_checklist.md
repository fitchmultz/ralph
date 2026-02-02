<!-- Purpose: Shared completion checklist injected into implementation-mode worker prompts. -->
## IMPLEMENTATION COMPLETION CHECKLIST
When implementation is complete, you MUST:
1. Investigate and resolve any risks, bugs, or suspicious leads you flagged during this run before completion. If a lead is a false positive, document why in your final response; do not complete the task otherwise.
2. Run `ralph task done --note "<note>" {{TASK_ID}}` to move the task from `.ralph/queue.json` to `.ralph/done.json`.
   - Use `ralph task reject --note "<note>" {{TASK_ID}}` when appropriate; only `done` and `rejected` are valid completion statuses.
   - Provide 1-5 summary notes using repeated `--note` flags (each note should be a short bullet).
   - **Queue freshness check (MANDATORY before marking done/rejected):** quickly scan other tasks in `.ralph/queue.json` (typically `todo` / `doing`) and identify any tasks whose **assumptions, plan, evidence, or notes** are now stale because of what you just changed (APIs, file paths, behavior, config, constraints, etc.).
     - If affected, update those tasks using `ralph task field <KEY> <VALUE> <TASK_ID>` to add clarifying notes so future agents aren't misled.
     - Prefer minimal, high-signal updates to eliminate confirmed stale data (e.g., `ralph task field stale_api "This API no longer exists; see <new path>" RQ-0XXX`).
3. If the task is incomplete but you are stopping:
   - Leave it in `.ralph/queue.json` as `doing` (or revert to `todo` if not continuing).
   - Do NOT set `blocked`.
4. Do NOT manually edit `.ralph/queue.json` or `.ralph/done.json` to complete tasks, and do not run `ralph queue archive` for single-task completion.
5. Ensure `.ralph/queue.json` remains valid JSON and respects the queue contract.
6. If the CI gate is enabled ({{config.agent.ci_gate_enabled}}), run `{{config.agent.ci_gate_command}}` and fix all failures before ending your turn.
7. Git hygiene:
   - Do NOT commit or push until `ralph task done` succeeds.
   - If auto commit/push is enabled ({{config.agent.git_commit_push_enabled}}), do NOT run `git commit` or `git push` manually; Ralph will commit/push after completion.
   - If auto commit/push is disabled ({{config.agent.git_commit_push_enabled}}), leave the repo dirty and report that manual commit/push is required.
   - Confirm repo state: when enabled, `git status --porcelain` is empty after completion; when disabled, note remaining changes.
   - If a push is required but cannot be performed (no upstream/permissions), stop and report the blocker.
