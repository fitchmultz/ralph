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
   - Plan-only violations prompt for action when `git_revert_mode=ask`; you can keep+proceed (explicit override), revert changes, or continue planning with a message.
2. Phase 2 (Implementation + CI): apply changes, run the configured CI gate command (default `make ci`) when enabled, then stop.
3. Phase 3 (Review + Completion): review diff, resolve any flagged risks or suspicious leads before completion, re-run the configured CI gate command (default `make ci`) when enabled, complete task, and (when auto git commit/push is enabled) commit and push.
   - With auto git commit/push enabled, Phase 3 requires a clean repo to finish; for rejected tasks, the only allowed dirty files are `.ralph/queue.json` and `.ralph/done.json` (queue bookkeeping).

Phases can be set via `--phases` or `agent.phases` in config.

## Security and Redaction

### Safeguard Dumps
When operations fail (runner errors, scan validation failures), Ralph writes safeguard dumps to temp directories for troubleshooting. These dumps are **redacted by default** to prevent secrets from being written to disk.

**Redaction applies to:**
- API keys and bearer tokens
- AWS access keys (AKIA...)
- SSH private keys
- Hex tokens (32+ characters)
- Sensitive environment variable values

**Raw dumps** are only written when explicitly opted in via:
- `RALPH_RAW_DUMP=1` environment variable
- `--debug` flag (implies verbose output desired)

### Debug Logging
When `--debug` is enabled, raw runner output is written to `.ralph/logs/debug.log`. This is intentional for troubleshooting but may contain unredacted secrets. Debug logs should be treated as sensitive and never committed.

## Runner Model Control
Runner and model selection are driven by a combination of CLI flags, task overrides, and config. The CLI has the highest priority for a single run.

## Session State

Session state is persisted to `.ralph/cache/session.json` for crash recovery. It includes:
- Task ID and session metadata
- Iteration and phase progress
- **Per-phase runner/model settings** (for display in recovery prompts)

Note: Per-phase settings are informational only. Crash recovery recomputes settings from CLI flags, config, and task overrides to ensure consistency.
