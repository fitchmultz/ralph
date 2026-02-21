# Workflow and Architecture

![3-Phase Workflow](assets/images/2026-02-07-workflow-3phase.png)

Purpose: Explain Ralph's high-level runtime layout, phases, and prompt override workflow without deep internals.

## Runtime Files
- `.ralph/queue.json`: source of truth for active tasks.
- `.ralph/done.json`: archive of completed tasks.
- `.ralph/config.json`: project-level configuration.
- `.ralph/prompts/*.md`: optional prompt overrides (defaults are embedded in the Rust CLI under `crates/ralph/assets/prompts/`).
- `.ralph/cache/parallel/state.json`: parallel run state (in-flight workers and terminal outcomes).

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
   - With auto git commit/push enabled, Phase 3 requires a clean repo to finish; for rejected tasks, allowed dirty files include `.ralph/queue.{json,jsonc}`, `.ralph/done.{json,jsonc}`, `.ralph/config.{json,jsonc}`, and `.ralph/cache/` (Ralph bookkeeping/state).

Phases can be set via `--phases` or `agent.phases` in config.

## Parallel Run Loop (CLI Only)

Parallel execution is available only via the CLI (`ralph run loop --parallel [N]`).

High-level behavior:
- Each task runs in its own isolated git workspace clone under
  `<repo-parent>/.workspaces/<repo-name>/parallel/<TASK_ID>` by default
  (configurable via `parallel.workspace_root`).
  Each workspace checks out the coordinator target base branch (for example `main`).
- Workers run configured phases, then execute an agent-owned integration loop:
  `fetch/rebase/conflict-fix/commit/push`.
- Workers push directly to `origin/<target_branch>`; no PR/merge-agent lifecycle is used.
- State is persisted to `.ralph/cache/parallel/state.json` for crash recovery and coordination.
- On startup, Ralph prunes stale worker records before evaluating the state file's target branch.
  If the target branch is missing or mismatched and there are no active workers, Ralph auto-heals
  the state file to the current branch.
  Otherwise it fails with recovery guidance to avoid mixing active parallel runs across branches.

**Breaking change (2026-02):** The default directory for parallel workspaces changed from
`.worktrees/` to `.workspaces/`. The config key `parallel.worktree_root` has been renamed to
`parallel.workspace_root` and is no longer accepted. Run `ralph migrate` to update existing
configs if you have custom `worktree_root` settings.

## Wait When Blocked (Sequential Loop)

When all remaining tasks are blocked by unmet dependencies (`depends_on`) or future schedules (`scheduled_start`), the sequential run loop normally exits with a summary of the blockers. Use `--wait-when-blocked` to keep the loop running and poll for changes instead.

Behavior:
- The loop polls `.ralph/queue.json` and `.ralph/done.json` for changes
- When a runnable task appears (dependencies complete or schedule passes), the loop continues
- Configurable poll interval (`--wait-poll-ms`, default: 1000ms, min: 50ms)
- Optional timeout (`--wait-timeout-seconds`, 0 = no timeout)
- Optional notification when unblocked (`--notify-when-unblocked`, desktop + webhook)
- Respects stop signals (`ralph queue stop`) and Ctrl+C

Use this for "fire and forget" execution through dependent task chains without manual babysitting.

Examples:
```bash
# Wait indefinitely for dependencies/schedules to resolve
ralph run loop --wait-when-blocked

# Wait with a 10-minute timeout and notify when unblocked
ralph run loop --wait-when-blocked --wait-timeout-seconds 600 --notify-when-unblocked
```

### Queue Unblocked Webhook Event

When using `--notify-when-unblocked` with webhooks configured, Ralph emits a `queue_unblocked` event:

- `previous_status`: `"blocked"`
- `current_status`: `"runnable"`
- `note`: Summary counts like `ready=2 blocked_deps=3 blocked_schedule=1`

This event is opt-in; add `"queue_unblocked"` to your webhook events list to receive it.

## Continuous Mode (Sequential Loop)

When the queue is empty, the sequential run loop normally exits. Use `--wait-when-empty` (alias `--continuous`) to keep the loop running and wait for new tasks instead.

Behavior:
- If the queue is empty at startup, the loop does not exit; it waits for work
- If the loop runs out of candidates later, it waits instead of exiting
- Uses filesystem notifications (`notify` crate) to watch `.ralph/queue.json` and `.ralph/done.json`
- Falls back to polling if notifications fail
- Configurable poll interval (`--empty-poll-ms`, default: 30000ms = 30s, min: 50ms)
- No timeout in continuous mode (runs until stopped)
- Respects stop signals (`ralph queue stop`) and Ctrl+C

Combined with `--wait-when-blocked`, the loop provides "always-on" operation that handles both blocked tasks and empty queues.

Use this for "set and forget" operation that integrates with system services (systemd, launchd).

Examples:
```bash
# Continuous mode: wait indefinitely for new tasks
ralph run loop --continuous

# Poll more frequently (5s) for faster response
ralph run loop --continuous --empty-poll-ms 5000

# Always-on mode: handle both blocked and empty states
ralph run loop --continuous --wait-when-blocked
```

### Daemon Mode

For background operation, use `ralph daemon start|stop|status` (Unix-only):

```bash
# Start daemon
ralph daemon start

# Check status
ralph daemon status

# View logs
ralph daemon logs
# Live follow
ralph daemon logs --follow

# Stop daemon
ralph daemon stop
```

The daemon is a thin wrapper around `ralph run loop --continuous --wait-when-blocked` that:
- Detaches from the terminal
- View logs with `ralph daemon logs`
- Manages PID/state files in `.ralph/cache/`
- Responds to `ralph daemon stop` and `ralph queue stop`

See `docs/cli.md` for systemd and launchd service templates.

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
- Ensure `.ralph/logs/` is in `.gitignore`
- Clean up debug logs after use: `rm -rf .ralph/logs/`

## Runner Model Control
Runner and model selection are driven by a combination of CLI flags, task overrides, and config. The CLI has the highest priority for a single run.

## Session State

Session state is persisted to `.ralph/cache/session.json` for crash recovery. It includes:
- Task ID and session metadata
- Iteration and phase progress
- **Per-phase runner/model settings** (for display in recovery prompts)

Note: Per-phase settings are informational only. Crash recovery recomputes settings from CLI flags, config, and task overrides to ensure consistency.

## Webhook Events

Ralph can emit webhook events for external integrations (Slack, Discord, CI systems, dashboards). Webhooks are configured via `agent.webhook` in config.

### Event Types

**Task Events** (enabled by default):
- `task_created`: Task added to queue
- `task_started`: Task execution begins
- `task_completed`: Task finished successfully
- `task_failed`: Task failed or was rejected
- `task_status_changed`: Generic status transition

**Loop Events** (opt-in):
- `loop_started`: Run loop begins (includes repo/branch/commit context)
- `loop_stopped`: Run loop ends (includes duration and summary)

**Phase Events** (opt-in):
- `phase_started`: Phase execution begins (includes runner/model/phase context)
- `phase_completed`: Phase execution ends (includes duration and CI gate outcome)

### Opt-in Behavior

New event types (`loop_*`, `phase_*`) are **opt-in** and not enabled by default. To receive these events, explicitly configure them:

```json
{
  "agent": {
    "webhook": {
      "enabled": true,
      "url": "https://example.com/webhook",
      "events": ["loop_started", "phase_started", "phase_completed", "loop_stopped"]
    }
  }
}
```

Use `["*"]` to subscribe to all events including new ones.

### Runner Session Handling (Kimi)

![Session Management](assets/images/2026-02-07-session-management.png)

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
