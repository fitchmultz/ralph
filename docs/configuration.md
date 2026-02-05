# Configuration

Purpose: Document Ralph's JSON configuration layout, defaults, and override precedence for global and project settings.

## Overview
Ralph reads JSON configuration from two locations, with project config taking precedence over global:
- Global: `~/.config/ralph/config.json`
- Project: `.ralph/config.json`

CLI flags override both for a single run. Defaults are defined by `schemas/config.schema.json`.

## JSONC Support (JSON with Comments)

Ralph supports JSONC (JSON with Comments) for configuration and queue files. This allows you to add comments to your config and task files for better documentation.

### Supported Comment Styles
- Single-line comments: `// This is a comment`
- Multi-line comments: `/* This is a multi-line comment */`
- Trailing commas in objects and arrays

### File Extensions
- `.json` - Standard JSON (default, backward compatible)
- `.jsonc` - JSON with Comments support

Ralph can read both `.json` and `.jsonc` files regardless of extension. When writing files, Ralph always outputs standard JSON format (comments are not preserved on rewrite).

### Example JSONC Config

```jsonc
{
  // Schema version - must be 1
  "version": 1,
  "agent": {
    /* Runner configuration
       Choose from: codex, opencode, gemini, claude, cursor */
    "runner": "claude",
    "phases": 3, // 1 = single-pass, 2 = plan+implement, 3 = plan+implement+review
  }
}
```

### Notes
- Schema files (`schemas/*.schema.json`) remain strict JSON for validator compatibility
- Comments are for human editing only; Ralph outputs standard JSON when saving

## Top-Level Fields
- `version` (number): Config schema version. Default: `1`.
- `project_type` (string or null): `code` or `docs`. Default: `code`.
- `agent` (object): Runner defaults (CLI binaries, runner, model, phases, and prompt enforcement).
- `parallel` (object): Parallel run-loop configuration for `ralph run loop` (CLI only).
- `queue` (object): Queue file locations and task ID formatting.
- `plugins` (object): Plugin configuration (enable/disable + per-plugin settings).

## Agent Configuration
`agent` controls default execution settings. Defaults are schema-defined.

Supported fields:
- `runner`: `codex`, `opencode`, `gemini`, `claude`, or `cursor`.
- `model`: default model id (string).
- `phases`: number of phases (1, 2, or 3).
- `update_task_before_run`: if `true`, Ralph runs `ralph task update <TASK-ID>` once per task immediately before execution begins (default: `false`). This updates task fields (scope, evidence, plan, notes, tags, depends_on) based on current repository state, priming agents with better task information. Runs only once per task, before the first iteration (not before subsequent iterations if `iterations > 1`). Can also be enabled via CLI flag: `--update-task`.
- `reasoning_effort`: `low`, `medium`, `high`, `xhigh` (Codex only).
- `iterations`: number of iterations to run per task (default: 1).
- `followup_reasoning_effort`: reasoning effort for iterations after the first (Codex only).
- `repoprompt_plan_required`: require RepoPrompt planning instructions (context_builder) during Phase 1.
- `repoprompt_tool_injection`: inject RepoPrompt tooling reminders into prompts.
- `git_revert_mode`: `ask`, `enabled`, or `disabled`.
- `git_commit_push_enabled`: enable or disable automatic git commit/push after successful runs (default: `true`).
  **Safety warning:** When enabled, Ralph will automatically push changes to the remote repository. This action is irreversible. The TUI will prompt for confirmation when enabling this setting.
  **Parallel workers:** This setting is inherited by parallel workers. When disabled, parallel PR automation (`auto_pr`, `auto_merge`, `draft_on_failure`) is skipped because PRs require pushed commits.
- `session_timeout_hours`: session timeout in hours for crash recovery (default: `24`). Sessions older than this threshold are considered stale and require explicit user confirmation to resume. Set to a higher value if you want to allow resuming sessions after longer periods.
- `ci_gate_command`: command to run for the CI gate (default: `make ci`).
- `ci_gate_enabled`: enable or disable the CI gate (default: `true`).
  **Safety warning:** Disabling the CI gate skips validation before commit/push, which may allow broken code to be pushed.
- `claude_bin`, `codex_bin`, `opencode_bin`, `gemini_bin`, `cursor_bin`: override runner executable path/name (Cursor uses the `agent` binary).
- `claude_permission_mode`: `accept_edits` or `bypass_permissions`.
  **Safety warning:** `bypass_permissions` allows Claude to make edits without prompting for approval. Use with caution.
- `runner_cli`: normalized runner CLI behavior (output/approval/sandbox/etc), with global defaults and optional per-runner overrides.
- `instruction_files`: optional list of additional instruction file paths to inject at the top of every prompt sent to runner CLIs (repo-root relative, absolute, or `~/`).

  To inject both global and repo-local AGENTS.md:

  ```json
  {
    "agent": {
      "instruction_files": ["~/.codex/AGENTS.md", "AGENTS.md"]
    }
  }
  ```

Notes:
- `followup_reasoning_effort` is ignored for non-Codex runners.
- Breaking change: `reasoning_effort` no longer accepts `minimal`; use `low`, `medium`, `high`, or `xhigh`.
- CI gate auto-retry: When enabled, Ralph automatically sends a strict compliance message and retries up to 2 times on CI failure during Phase 2, Phase 3, or single-phase execution. This behavior is not configurable; after 2 automatic retries, the user is prompted via the configured `git_revert_mode`. Post-run supervision prompts immediately on CI failure.
- Phase 1 plan-only violations: when `git_revert_mode=ask`, the prompt includes a keep+continue override to proceed to the next phase without reverting changes.
- **Runner session handling**: For runners that support session resumption (e.g., Kimi), Ralph generates unique session IDs per phase (format: `{task_id}-p{phase}-{timestamp}`) and uses explicit `--session` flags rather than runner-specific continue mechanisms. This provides deterministic session management and reliable crash recovery.

### `agent.runner_cli`

`agent.runner_cli` provides a normalized configuration surface for runner CLI behavior so Ralph can keep parity across runners while still emitting runner-specific flags.

Structure:
- `agent.runner_cli.defaults`: applied to all runners (unless overridden)
- `agent.runner_cli.runners.<runner>`: per-runner overrides (merged leaf-wise over `defaults`)

Supported normalized fields:
- `output_format`: `stream_json`, `json`, `text` (execution requires `stream_json`)
- `verbosity`: `quiet`, `normal`, `verbose`
- `approval_mode`: `default`, `auto_edits`, `yolo`, `safe`
  **Safety warning:** `yolo` mode bypasses all approval prompts, allowing the runner to make changes without confirmation. Use with extreme caution.
  
  **Codex exception**: Ralph does NOT pass approval flags to Codex, regardless of this setting. Codex will use whatever approval mode is configured in your global Codex config file (`~/.codex/config.json`). If you want YOLO behavior with Codex, configure it there, not in Ralph.
- `sandbox`: `default`, `enabled`, `disabled`
- `plan_mode`: `default`, `enabled`, `disabled`
- `unsupported_option_policy`: `ignore`, `warn`, `error`

Notes:
- Unsupported options are dropped by default with a warning (policy `warn`).
- `agent.claude_permission_mode` remains supported; when `runner_cli.approval_mode` is set, it takes precedence for Claude mapping.
Example:

```json
{
  "version": 1,
  "agent": {
    "runner": "codex",
    "model": "gpt-5.3-codex",
    "phases": 3,
    "iterations": 2,
    "reasoning_effort": "high",
    "followup_reasoning_effort": "low",
    "repoprompt_plan_required": false,
    "repoprompt_tool_injection": false,
    "git_commit_push_enabled": true,
    "git_revert_mode": "ask",
    "claude_permission_mode": "bypass_permissions",
    "runner_cli": {
      "defaults": {
        "output_format": "stream_json",
        "approval_mode": "yolo",
        "unsupported_option_policy": "warn"
      },
      "runners": {
        "codex": { "sandbox": "disabled" },
        "claude": { "verbosity": "verbose" }
      }
    },
    "ci_gate_command": "make ci",
    "ci_gate_enabled": true
  }
}
```

To disable CI gating entirely (skip running any command), set:

```json
{
  "agent": {
    "ci_gate_enabled": false
  }
}
```

To configure a longer session timeout for crash recovery (e.g., 72 hours for weekend-long tasks):

```json
{
  "agent": {
    "session_timeout_hours": 72
  }
}
```

### `agent.phase_overrides`

Optional. Per-phase overrides for runner, model, and reasoning effort. Allows using different runners or models for different phases of task execution.

**Structure:**
- `phase1` - Overrides for phase 1 (planning)
- `phase2` - Overrides for phase 2 (implementation)
- `phase3` - Overrides for phase 3 (review)

Each phase config can specify:
- `runner` - Override the runner (e.g., "codex", "claude")
- `model` - Override the model (e.g., "o3-mini", "claude-opus-4")
- `reasoning_effort` - Override reasoning effort ("low", "medium", "high", "xhigh")

**Example:**

```json
{
  "agent": {
    "runner": "codex",
    "model": "gpt-5.3-codex",
    "reasoning_effort": "medium",
    "phase_overrides": {
      "phase1": {
        "model": "gpt-5.2",
        "reasoning_effort": "high"
      },
      "phase2": {
        "runner": "kimi",
        "model": "kimi-code/kimi-for-coding"
      },
      "phase3": {
        "runner": "codex",
        "model": "gpt-5.3-codex",
        "reasoning_effort": "high"
      }
    }
  }
}
```

**Precedence (per phase):** CLI phase flags > config phase override (`agent.phase_overrides.phaseN.*`) > CLI global overrides > task overrides (`task.agent.*`) > config defaults (`agent.*`) > code defaults

## Parallel Configuration

`parallel` controls the CLI-only parallel execution mode for `ralph run loop`. The TUI does not
support parallel runs.

Key fields:
- `workers`: number of concurrent workers (must be `>= 2`). Default: `null` (disabled unless CLI
  `--parallel` is used).
- `merge_when`: `as_created` (default) or `after_all` to merge PRs as they are created or only after
  all tasks finish.
- `merge_method`: `squash` (default), `merge`, or `rebase`.
- `auto_pr`: automatically create PRs for completed tasks (default: `true`).
- `auto_merge`: automatically merge PRs when eligible (default: `true`).
- `draft_on_failure`: create draft PRs when a worker fails (default: `true`).
- `conflict_policy`: `auto_resolve` (default), `retry_later`, or `reject`.
- `merge_retries`: number of merge retries before giving up (default: `5`).
- `workspace_root`: root directory for parallel workspaces (default: `<repo-parent>/.workspaces/<repo-name>/parallel`).

  **Git hygiene warning:** If you set `parallel.workspace_root` to a path **inside** the repository (for example `.ralph/workspaces`), you MUST gitignore it (or add it to `.git/info/exclude`). Otherwise Ralph will create workspace clone directories that appear as untracked files and the repo will look "dirty" across runs. Parallel mode will fail fast if the workspace root is inside the repo and not ignored.

- `branch_prefix`: prefix for worker branches (default: `ralph/`).

  **Important:** The auto-merge feature expects PR head branch names to be exactly
  `{branch_prefix}{task_id}`. If a PR's head branch doesn't match this pattern, the
  merge runner will skip it and persist a `merge_blocker` warning in the parallel
  state file (`.ralph/cache/parallel/state.json`). This prevents accidental merges
  when branch naming conventions change or PRs are created externally.

  Recovery: If you change `branch_prefix`, existing open PRs created under the old
  prefix will be blocked with a persisted state warning until you either:
  1. Rename the PR branches to match the new prefix, or
  2. Manually clear the `merge_blocker` field from the state file.
- `delete_branch_on_merge`: delete branches after merge (default: `true`).
- `merge_runner`: runner overrides for merge conflict resolution (defaults to `agent` settings).

Notes:
- CLI flag `--parallel` overrides `parallel.workers` for a single run.
- If `auto_pr` is `false`, PR creation and merge automation are skipped.
- `auto_pr`, `auto_merge`, and `draft_on_failure` require `agent.git_commit_push_enabled` (or CLI `--git-commit-push-on`) to be enabled, since PRs require pushed commits. When commit/push is disabled, these settings are automatically disabled for the invocation.
- When PR automation is disabled or PR creation fails, the coordinator records the task as finished without a PR in `.ralph/cache/parallel/state.json` and will not re-run it in parallel mode until the entry is cleared. If the task already completed successfully, mark it done manually since no PR exists for the coordinator to apply.
- **Breaking change (2026-02):** The `parallel.worktree_root` config key has been renamed to
  `parallel.workspace_root`. Config files using the old key will fail to load. Run
  `ralph migrate` to update existing configs. State files are not migrated and may need
  to be deleted if incompatible.

Example:

```json
{
  "parallel": {
    "workers": 3,
    "merge_when": "as_created",
    "merge_method": "squash",
    "conflict_policy": "auto_resolve",
    "merge_retries": 5,
    "branch_prefix": "ralph/",
    "merge_runner": {
      "runner": "claude",
      "model": "sonnet",
      "reasoning_effort": "medium"
    }
  }
}
```

## Queue Configuration
`queue` controls file locations and task ID formatting.

Supported fields:
- `file`: path to the queue file (default: `.ralph/queue.json`).
- `done_file`: path to the done archive (default: `.ralph/done.json`).
- `id_prefix`: task ID prefix (default: `RQ`).
- `id_width`: zero padding width (default: `4`, e.g. `RQ-0001`).

**Parallel mode restriction:** When running `ralph run loop --parallel ...`, `queue.file` and
`queue.done_file` must resolve to paths **under the repository root**. Parallel mode maps these
paths into per-worker workspace clones; paths outside the repo root cannot be mapped safely and are
rejected during parallel preflight. Prefer repo-relative paths like `.ralph/queue.json` and
`.ralph/done.json`.

Example:

```json
{
  "version": 1,
  "queue": {
    "file": ".ralph/queue.json",
    "done_file": ".ralph/done.json",
    "id_prefix": "RQ",
    "id_width": 4
  }
}
```

## Precedence
1. CLI flags (single run)
2. Project config (`.ralph/config.json`)
3. Global config (`~/.config/ralph/config.json`)
4. Schema defaults (`schemas/config.schema.json`)

## TUI Safety Warnings

When using the TUI config editor (`e` key in the task list), certain high-risk settings display inline warnings:

- **Danger level** (⚠): Settings like `git_commit_push_enabled` that can cause irreversible actions. The TUI will prompt for confirmation before enabling these.
- **Warning level** (ℹ): Settings like `approval_mode` and `claude_permission_mode` that reduce safety checks. These show descriptive text but don't require confirmation.

The confirmation dialog for Danger-level settings explains the risk and requires an explicit `y` keypress to proceed. Pressing `n` or `Esc` cancels the change.

## Notification Configuration

`agent.notification` controls desktop notifications for task completion and failures.

Supported fields:
- `enabled`: legacy field, enable/disable all notifications (default: `true`).
- `notify_on_complete`: enable notifications on task completion (default: `true`).
- `notify_on_fail`: enable notifications on task failure (default: `true`).
- `notify_on_loop_complete`: enable notifications when loop mode finishes (default: `true`).
- `suppress_when_active`: suppress notifications when TUI is active (default: `true`).
- `sound_enabled`: play sound with notification (default: `false`).
- `sound_path`: custom sound file path (optional, platform-specific).
- `timeout_ms`: notification display duration in milliseconds (default: `8000`, range: `1000-60000`).

Platform notes:
- **macOS**: Uses NotificationCenter; sound plays via `afplay` (default: `/System/Library/Sounds/Glass.aiff`).
- **Linux**: Uses D-Bus/notify-send; sound plays via `paplay`/`aplay` or `canberra-gtk-play` for default sounds.

## Webhook Configuration

`agent.webhook` controls HTTP webhook notifications for task events. Webhooks complement desktop notifications by enabling external integrations (Slack, Discord, CI systems, etc.) to receive real-time task events.

Supported fields:
- `enabled`: enable webhook notifications (default: `false`).
- `url`: webhook endpoint URL (required when enabled).
- `secret`: secret key for HMAC-SHA256 signature generation (optional).
  When set, webhooks include an `X-Ralph-Signature` header for verification.
- `events`: list of events to subscribe to (default: all events).
  - Supported: `task_created`, `task_started`, `task_completed`, `task_failed`, `task_status_changed`
  - Use `["*"]` to subscribe to all events
- `timeout_secs`: request timeout in seconds (default: `30`, max: `300`).
- `retry_count`: number of retry attempts for failed deliveries (default: `3`, max: `10`).
- `retry_backoff_ms`: retry backoff base in milliseconds (default: `1000`, max: `30000`).
- `queue_capacity`: maximum number of pending webhooks in the delivery queue (default: `100`, range: `10-10000`).
- `queue_policy`: backpressure policy when queue is full (default: `drop_oldest`).
  - `drop_oldest`: Drop new webhooks when queue is full (preserves existing queue contents).
  - `drop_new`: Drop the new webhook if the queue is full.
  - `block_with_timeout`: Briefly block the caller (100ms), then drop if queue is still full.

### Delivery Semantics

Webhooks are delivered **asynchronously** by a background worker thread:

- **Best-effort delivery**: Webhooks may be dropped if the queue is full (per `queue_policy`).
- **Non-blocking**: The `send_webhook` call returns immediately after enqueueing.
- **Order preservation**: Webhooks are delivered in FIFO order within the constraints of the backpressure policy.
- **Failure handling**: Failed deliveries are retried (per `retry_count` and `retry_backoff_ms`).
- **Worker lifecycle**: The background worker starts on first webhook send and shuts down when the process exits.

Example:

```json
{
  "version": 1,
  "agent": {
    "webhook": {
      "enabled": true,
      "url": "https://hooks.slack.com/services/T00000000/B00000000/XXXXXXXXXXXXXXXXXXXXXXXX",
      "secret": "my-webhook-secret",
      "events": ["task_completed", "task_failed"],
      "timeout_secs": 30,
      "retry_count": 3,
      "retry_backoff_ms": 1000,
      "queue_capacity": 100,
      "queue_policy": "drop_oldest"
    }
  }
}
```

### Webhook Payload Format

Webhooks are sent as HTTP POST requests with JSON payloads:

```json
{
  "event": "task_completed",
  "timestamp": "2024-01-15T10:30:00Z",
  "task_id": "RQ-0001",
  "task_title": "Add webhook support",
  "previous_status": "doing",
  "current_status": "done",
  "note": null
}
```

### Webhook Security

When a `secret` is configured, webhooks include an `X-Ralph-Signature` header:

```
X-Ralph-Signature: sha256=abc123...
```

The signature is computed as HMAC-SHA256 of the request body using the configured secret.

To verify in Python:

```python
import hmac
import hashlib

secret = b'my-webhook-secret'
body = request.body

expected_signature = 'sha256=' + hmac.new(
    secret, body, hashlib.sha256
).hexdigest()

if not hmac.compare_digest(
    expected_signature,
    request.headers.get('X-Ralph-Signature', '')
):
    raise ValueError("Invalid signature")
```

### Testing Webhooks

Use the CLI to test your webhook configuration:

```bash
# Test with configured URL
ralph webhook test

# Test with specific event type
ralph webhook test --event task_completed

# Test with custom URL
ralph webhook test --url https://example.com/webhook
```

- **Windows**: Uses toast notifications; custom sounds play via `winmm.dll` PlaySound for `.wav` files, PowerShell MediaPlayer fallback for other formats.

Example:

```json
{
  "version": 1,
  "agent": {
    "notification": {
      "enabled": true,
      "notify_on_complete": true,
      "notify_on_fail": true,
      "notify_on_loop_complete": true,
      "suppress_when_active": true,
      "sound_enabled": true,
      "timeout_ms": 10000
    }
  }
}
```

CLI overrides:
- `--notify`: Enable notification on task completion (overrides config).
- `--no-notify`: Disable notification on task completion (overrides config).
- `--notify-fail`: Enable notification on task failure (overrides config).
- `--no-notify-fail`: Disable notification on task failure (overrides config).
- `--notify-sound`: Enable sound for this run (works with notification flags or when enabled in config).

## Plugin Configuration

`plugins` controls custom runner and processor plugins. Plugins enable extending Ralph with custom runners without modifying the core codebase.

**Security warning:** Plugins are NOT sandboxed. Enabling a plugin is equivalent to trusting it with full system access. Only enable plugins from trusted sources.

Supported fields:
- `plugins.plugins.<id>.enabled`: enable/disable the plugin (default: `false`).
- `plugins.plugins.<id>.runner.bin`: override the runner executable path.
- `plugins.plugins.<id>.processor.bin`: override the processor executable path.
- `plugins.plugins.<id>.config`: opaque configuration blob passed to the plugin.

Plugin directories (searched in order, project overrides global):
- Project: `.ralph/plugins/<plugin_id>/plugin.json`
- Global: `~/.config/ralph/plugins/<plugin_id>/plugin.json`

Example:

```json
{
  "version": 1,
  "plugins": {
    "plugins": {
      "my.custom-runner": {
        "enabled": true,
        "runner": {
          "bin": "custom-runner"
        },
        "config": {
          "api_key": "secret",
          "endpoint": "https://api.example.com"
        }
      }
    }
  }
}
```

Plugin management commands:
- `ralph plugin list`: List discovered plugins
- `ralph plugin validate`: Validate plugin manifests
- `ralph plugin install <path> --scope project|global`: Install a plugin
- `ralph plugin uninstall <id> --scope project|global`: Uninstall a plugin

See [Plugin Development Guide](./plugin-development.md) for creating custom plugins.
