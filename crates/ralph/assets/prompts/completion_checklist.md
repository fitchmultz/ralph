<!-- Purpose: Shared completion checklist injected into implementation-mode worker prompts. -->
## IMPLEMENTATION COMPLETION CHECKLIST
Run mode for this session: `{{RUN_MODE}}` (expected values: `normal`, `parallel-worker`).

When implementation is complete, you MUST:
1. Investigate and resolve any risks, bugs, or suspicious leads you flagged during this run before completion. If a lead is a false positive, document why in your final response; do not complete the task otherwise.
2. Finalize task bookkeeping:
   - If `RUN_MODE=normal`: run `ralph task done --note "<note>" {{TASK_ID}}` to move the task from queue to done.
   - If `RUN_MODE=parallel-worker`: do NOT run `ralph task done`; during integration, update workspace queue/done files (which are pushed to the base branch) and ensure `{{TASK_ID}}` is removed from queue and present in done.
   - Use `ralph task reject --note "<note>" {{TASK_ID}}` when appropriate in non-parallel flows; only `done` and `rejected` are valid completion statuses.
   - Provide 1-5 summary notes using repeated `--note` flags (each note should be a short bullet) when using `ralph task done`/`reject`.
   - **Queue freshness check (MANDATORY before marking done/rejected):** quickly scan other tasks in `{{config.queue.file}}` (typically `todo` / `doing`) and identify any tasks whose **assumptions, plan, evidence, or notes** are now stale because of what you just changed (APIs, file paths, behavior, config, constraints, etc.).
     - If affected, update those tasks using `ralph task field <KEY> <VALUE> <TASK_ID>` to add clarifying notes so future agents aren't misled.
     - Prefer minimal, high-signal updates to eliminate confirmed stale data (e.g., `ralph task field stale_api "This API no longer exists; see <new path>" RQ-0XXX`).
3. If the task is incomplete but you are stopping:
   - Leave it in `{{config.queue.file}}` as `doing` (or revert to `todo` if not continuing).
   - Do NOT set `blocked`.
4. Do NOT run `ralph queue archive` for single-task completion.
   - If `RUN_MODE=normal`: do NOT manually edit queue/done files.
   - If `RUN_MODE=parallel-worker`: queue/done updates are part of the integration contract; preserve other workers' entries exactly.
5. Ensure `{{config.queue.file}}` remains valid queue JSON/JSONC and respects the queue contract.
6. CI Gate (Conditional):
   - BEFORE running the CI gate, check if you made ANY modifications during this phase using git status/diff.
   - If you made NO changes (only reviewed/validated): you MAY skip the CI gate even if enabled.
   - If you made ANY modifications: you MUST run the CI gate if enabled (`{{config.agent.ci_gate_display}}`) and fix all failures before ending your turn.
7. Git hygiene:
   - If `RUN_MODE=normal`: do NOT commit or push until `ralph task done` succeeds.
   - If `RUN_MODE=parallel-worker`: complete integration bookkeeping validation before commit/push.
   - If auto commit/push is enabled ({{config.agent.git_commit_push_enabled}}), do NOT run `git commit` or `git push` manually; Ralph will commit/push after completion.
   - If auto commit/push is disabled ({{config.agent.git_commit_push_enabled}}), leave the repo dirty and report that manual commit/push is required.
   - Confirm repo state: when enabled, `git status --porcelain` is empty after completion; when disabled, note remaining changes.
   - If a push is required but cannot be performed (no upstream/permissions), stop and report the blocker.
