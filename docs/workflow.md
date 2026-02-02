# Workflow and Architecture

Purpose: Explain Ralph's high-level runtime layout, phases, and prompt override workflow without deep internals.

## Runtime Files
- `.ralph/queue.json`: source of truth for active tasks.
- `.ralph/done.json`: archive of completed tasks.
- `.ralph/config.json`: project-level configuration.
- `.ralph/prompts/*.md`: optional prompt overrides (defaults are embedded in the Rust CLI under `crates/ralph/assets/prompts/`).
- `.ralph/cache/parallel/state.json`: parallel run state (in-flight tasks and PRs).

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
- `.ralph/prompts/merge_conflicts.md`
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

## Parallel Run Loop (CLI Only)

Parallel execution is available only via the CLI (`ralph run loop --parallel [N]`). The TUI does
not support parallel runs.

High-level behavior:
- Each task runs in its own git worktree under `.ralph/worktrees/parallel/<TASK_ID>` on a branch
  named `ralph/<TASK_ID>`.
- The supervisor creates PRs on success (draft PRs on failure when enabled).
- The merge runner merges PRs as they are created (or after all tasks), and can auto-resolve
  conflicts using the `merge_conflicts` prompt and `parallel.merge_runner` overrides.
- State is persisted to `.ralph/cache/parallel/state.json` for crash recovery and coordination.

## Security and Redaction

### Safeguard Dumps
When operations fail (runner errors, scan validation failures), Ralph writes safeguard dumps to temp directories for troubleshooting. These dumps are **redacted by default** to prevent secrets from being written to disk.

**Important**: Redaction is pattern-based and best-effort. It may miss secrets in unexpected formats, encoded data, or novel patterns. Always review dumps before sharing.

**Redaction applies to:**
- API keys and bearer tokens
- AWS access keys (AKIA...)
- SSH private keys
- Hex tokens (32+ characters)
- Sensitive environment variable values

**Raw dumps** are only written when explicitly opted in via:
- `RALPH_RAW_DUMP=1` or `RALPH_RAW_DUMP=true` environment variable
- `--debug` flag (implies verbose output desired; also enables raw debug logs)

**Never commit safeguard dumps** to version control, even when redacted.

### Debug Logging
When `--debug` is enabled, raw runner output is written to `.ralph/logs/debug.log`. This is intentional for troubleshooting but may contain unredacted secrets captured before redaction is applied. 

**Important:** Console output is redacted via `RedactedLogger`, but debug logs capture raw log records and runner streams before redaction. Debug logs should be treated as highly sensitive and never committed.

**Best practices:**
- Only use `--debug` when necessary for troubleshooting
- Treat `.ralph/logs/debug.log` as sensitive data
- Add `.ralph/logs/` to `.gitignore`
- Clean up debug logs after use: `rm -rf .ralph/logs/`

## Runner Model Control
Runner and model selection are driven by a combination of CLI flags, task overrides, and config. The CLI has the highest priority for a single run.

## Session State

Session state is persisted to `.ralph/cache/session.json` for crash recovery. It includes:
- Task ID and session metadata
- Iteration and phase progress
- **Per-phase runner/model settings** (for display in recovery prompts)

Note: Per-phase settings are informational only. Crash recovery recomputes settings from CLI flags, config, and task overrides to ensure consistency.

### Runner Session Handling (Kimi)

Ralph uses explicit session management for runners that support it (notably **Kimi**):

**Session ID Generation**
- Format: `{task_id}-p{phase}-{timestamp}`
- Example: `RQ-0001-p2-1704153600`
- Each phase (1, 2, 3) gets its own unique session ID

**Why Explicit Sessions?**
- **Deterministic**: Same ID always resumes the same session
- **Reliable**: No dependency on parsing JSON output or runner-specific `last_session_id` tracking
- **Debuggable**: Human-readable IDs make it easy to trace session lifecycle
- **Isolated**: Each phase has its own session, preventing context leakage between planning, implementation, and review

**Command Examples**
```bash
# Phase 2 initial invocation
kimi --print --output-format stream-json --model kimi-for-coding \
  --session RQ-0001-p2-1704153600 \
  --prompt "Implement the plan..."

# Phase 2 continue (CI failure retry)
kimi --print --output-format stream-json --model kimi-for-coding \
  --session RQ-0001-p2-1704153600 \
  --prompt "Fix the CI errors..."
```

**Implementation Notes**
- Ralph generates the session ID at phase start and reuses it for all continue operations within that phase
- The session ID is stored in `ContinueSession` for CI gate retry loops
- If Kimi crashes and the session becomes corrupted, Ralph will attempt to resume with the same ID (user accepts this risk)
