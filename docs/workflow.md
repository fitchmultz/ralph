# Workflow and Architecture

Purpose: Explain Ralph's high-level runtime layout, phases, and prompt override workflow without deep internals.

## Runtime Files
- `.ralph/queue.json`: source of truth for active tasks.
- `.ralph/done.json`: archive of completed tasks.
- `.ralph/config.json`: project-level configuration.
- `.ralph/prompts/*.md`: optional prompt overrides (defaults are embedded in the Rust CLI).

## Prompt Overrides
Ralph embeds default prompts in the Rust binary. To override prompts per repo, add:
- `.ralph/prompts/worker.md`
- `.ralph/prompts/task_builder.md`
- `.ralph/prompts/scan.md`

Overrides must preserve required placeholders (for example `{{USER_REQUEST}}` in task builder prompts).

## Three-Phase Workflow
Default execution uses three phases:
1. Phase 1 (Planning): plan is cached at `.ralph/cache/plans/<TASK_ID>.md`.
2. Phase 2 (Implementation + CI): apply changes, run `make ci`, then stop.
3. Phase 3 (Review + Completion): review diff, re-run `make ci`, complete task, commit, and push.

Phases can be set via `--phases` or `agent.phases` in config.

## Runner Model Control
Runner and model selection are driven by a combination of CLI flags, task overrides, and config. The CLI has the highest priority for a single run.
