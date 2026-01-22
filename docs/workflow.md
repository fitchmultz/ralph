# Workflow and Architecture

Purpose: Explain Ralph's high-level runtime layout, phases, and prompt override workflow without deep internals.

## Runtime Files
- `.ralph/queue.json`: source of truth for active tasks.
- `.ralph/done.json`: archive of completed tasks.
- `.ralph/config.json`: project-level configuration.
- `.ralph/prompts/*.md`: optional prompt overrides (defaults are embedded in the Rust CLI under `crates/ralph/assets/prompts/`).

## Prompt Overrides
Ralph embeds default prompts in the Rust binary. To override prompts per repo, add:
- `.ralph/prompts/worker.md` (base worker prompt)
- `.ralph/prompts/worker_phase1.md` (Phase 1 planning wrapper)
- `.ralph/prompts/worker_phase2.md` (Phase 2 implementation wrapper, 2-phase)
- `.ralph/prompts/worker_phase2_handoff.md` (Phase 2 handoff wrapper, 3-phase)
- `.ralph/prompts/worker_phase3.md` (Phase 3 review wrapper)
- `.ralph/prompts/worker_single_phase.md` (single-pass wrapper)
- `.ralph/prompts/completion_checklist.md`
- `.ralph/prompts/phase2_handoff_checklist.md`
- `.ralph/prompts/code_review.md`
- `.ralph/prompts/task_builder.md`
- `.ralph/prompts/scan.md`

Overrides must preserve required placeholders (for example `{{USER_REQUEST}}` in task builder prompts).

## Three-Phase Workflow
Default execution uses three phases:
1. Phase 1 (Planning): plan is cached at `.ralph/cache/plans/<TASK_ID>.md`.
2. Phase 2 (Implementation + CI): apply changes, run the configured CI gate command (default `make ci`) when enabled, then stop.
3. Phase 3 (Review + Completion): review diff, re-run the configured CI gate command (default `make ci`) when enabled, complete task, commit, and push.

Phases can be set via `--phases` or `agent.phases` in config.

## Runner Model Control
Runner and model selection are driven by a combination of CLI flags, task overrides, and config. The CLI has the highest priority for a single run.
