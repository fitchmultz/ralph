<!-- Purpose: Shared completion checklist injected into implementation-mode worker prompts. -->
## IMPLEMENTATION COMPLETION CHECKLIST
Run mode for this session: `{{RUN_MODE}}` (expected values: `normal`, `parallel-worker`).

Follow this checklist. Items marked REQUIRED are machine-enforced or integration-critical.

1. PREFERRED: investigate and resolve any risks, bugs, or suspicious leads flagged during this run before completion. If a lead is a false positive, document why in your final response.
2. Finalize task bookkeeping:
   - REQUIRED (`RUN_MODE=normal`): run `ralph task done --note "<note>" {{TASK_ID}}` or `ralph task reject --note "<note>" {{TASK_ID}}` for terminal state.
   - REQUIRED (`RUN_MODE=parallel-worker`): do not run `ralph task done`; Ralph reconciles queue/done bookkeeping after your integration turn.
   - PREFERRED: provide 1-5 short summary notes using repeated `--note` flags when using `ralph task done` or `ralph task reject`.
   - PREFERRED: quickly scan other tasks in `{{config.queue.file}}` and refresh clearly stale assumptions, notes, or evidence when your changes invalidate them.
3. If the task is incomplete but you are stopping:
   - REQUIRED: leave it in `{{config.queue.file}}` as `doing` (or revert to `todo` if not continuing).
   - REQUIRED: do not set `blocked`.
4. REQUIRED:
   - do not run `ralph queue archive` for single-task completion
   - if `RUN_MODE=normal`, do not manually edit queue/done files
   - if `RUN_MODE=parallel-worker`, do not manually rewrite queue/done files unless resolving conflict markers during rebase
5. REQUIRED: ensure `{{config.queue.file}}` remains valid queue JSON/JSONC and respects the queue contract.
6. CI Gate:
   - if you made no changes, you may skip the CI gate
   - REQUIRED: if you made changes and the CI gate is enabled, run `{{config.agent.ci_gate_display}}` and fix all failures before ending your turn
7. Git hygiene:
   - REQUIRED: if `{{config.agent.git_publish_mode}}` is `commit_and_push`, do not run `git commit` or `git push` manually; Ralph handles publish.
   - REQUIRED: if `RUN_MODE=parallel-worker`, leave the workspace rebased, conflict-free, committed, and CI-clean; Ralph will validate bookkeeping and push.
   - PREFERRED: report the final repo state clearly when manual follow-up is still required.
