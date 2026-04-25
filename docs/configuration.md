# Configuration
Status: Active
Owner: Maintainers
Source of truth: this document for its stated scope
Parent: [Ralph Documentation](index.md)


![Configuration Layers](assets/images/2026-03-10-12-00-08-config-layers.png)

Purpose: Document Ralph's JSON configuration layout, defaults, and override precedence for global and project settings.

## Overview
Ralph reads JSON configuration from two locations, with project config taking precedence over global:
- Global: `~/.config/ralph/config.jsonc`
- Project: `.ralph/config.jsonc`

CLI flags override both for a single run. Defaults are defined by `schemas/config.schema.json`.

## Repo execution trust

Project `.ralph/config.jsonc` may define execution-sensitive settings (for example `agent.*_bin`, plugin runner IDs, `agent.ci_gate`, and `plugins.*`). Ralph applies those project-layer values only when the repository is explicitly marked trusted via a **local-only** `.ralph/trust.jsonc` file. Legacy `.ralph/trust.json` is ignored. `trusted_at` is optional in the file; `allow_project_commands: true` is what marks the repo trusted.

**Supported ways to create the trust file (explicit opt-in):**

- **`ralph config trust init`** — Preferred for existing repos. Creates `.ralph/` if needed, then creates or merges `.ralph/trust.jsonc` with `allow_project_commands: true` and a `trusted_at` RFC3339 UTC timestamp when the file is missing. If the file already marks the repo trusted (both flags set), the command leaves the file byte-for-byte unchanged. If `allow_project_commands` is true but `trusted_at` is absent, the file is updated to add a timestamp.
- **`ralph init --trust-project-commands`** (alias **`--trust`**) — Runs the normal init scaffold, resolves configuration without enforcing trust until files exist, then writes the same trust file. Use when bootstrapping a new Ralph layout and you want trust created in the same step.

Ralph prints a short warning before writing or changing the trust file. **Do not commit** `.ralph/trust.jsonc`; keep it untracked (see repository `AGENTS.md`).

Manual example:

```jsonc
{
  "allow_project_commands": true,
  "trusted_at": "2026-04-19T00:00:00Z"
}
```

## JSONC Support (JSON with Comments)

Ralph supports JSONC (JSON with Comments) for configuration and queue files. This allows you to add comments to your config and task files for better documentation.

### Supported Comment Styles
- Single-line comments: `// This is a comment`
- Multi-line comments: `/* This is a multi-line comment */`
- Trailing commas in objects and arrays

### File Extensions
- `.jsonc` - JSON with Comments support for runtime config and queue files
- `.json` - Standard JSON used only where a strict JSON contract is required, such as schemas

When writing files, Ralph always outputs standard JSON format (comments are not preserved on rewrite).

### Example JSONC Config

```jsonc
{
  // Schema version - must be 2
  "version": 2,
  "agent": {
    /* Runner configuration.
       Built-in runner IDs: codex, opencode, gemini, claude, cursor, kimi, pi.
       Plugin runner IDs are also supported as non-empty strings. */
    "runner": "codex",
    "model": "gpt-5.4",
    "phases": 3, // 1 = single-pass, 2 = plan+implement, 3 = plan+implement+review
  }
}
```

### Notes
- Schema files (`schemas/*.schema.json`) remain strict JSON for validator compatibility
- Comments are for human editing only; Ralph outputs standard JSON when saving

## Top-Level Fields
- `version` (number): Config schema version. Default: `2`.
- `project_type` (string or null): `code` or `docs`. Default: `code`.
- `agent` (object): Runner defaults (CLI binaries, runner, model, phases, and prompt enforcement).
- `parallel` (object): Parallel run-loop configuration for `ralph run loop` and RalphMac Run Control loop launches.
- `queue` (object): Queue file locations and task ID formatting.
- `plugins` (object): Plugin configuration (enable/disable + per-plugin settings).
- `profiles` (object, optional): Named configuration profiles for quick workflow switching. Ralph also ships built-in `safe` and `power-user` profiles. See [Profiles](#profiles) below.

## Agent Configuration
`agent` controls default execution settings. Defaults are schema-defined.

Supported fields:
- `runner`: Built-in runner ID (`codex`, `opencode`, `gemini`, `claude`, `cursor`, `kimi`, or `pi`) or plugin runner ID.
- `model`: default model id (string).
- `phases`: number of phases (1, 2, or 3).
- `reasoning_effort`: `low`, `medium`, `high`, `xhigh` (Codex and Pi only).
- `iterations`: number of iterations to run per task (default: 1).
- `followup_reasoning_effort`: reasoning effort for iterations after the first (Codex and Pi only).
- `repoprompt_plan_required`: inject RepoPrompt planning guidance (favoring `context_builder` when available) during Phase 1.
- `repoprompt_tool_injection`: inject RepoPrompt tooling guidance into prompts when that environment is enabled.
- `git_revert_mode`: `ask`, `enabled`, or `disabled`.
- `git_publish_mode`: automatic git publish behavior after successful runs. Supported values: `off`, `commit`, `commit_and_push` (default: `off`).
  **Safety note:** `commit_and_push` has the highest blast radius because it publishes to the remote repository automatically. Prefer `off` or `commit` unless you explicitly want automated publishing.
  **Parallel workers:** Parallel workers inherit this setting inside each workspace. Parallel execution remains experimental.
- `session_timeout_hours`: session timeout in hours for crash recovery (default: `24`). Sessions older than this threshold are considered stale and require explicit user confirmation to resume. Set to a higher value if you want to allow resuming sessions after longer periods.
- `runner_retry`: runner invocation retry/backoff configuration for transient failure handling. See [`agent.runner_retry`](#agentrunner_retry) below.
- `ci_gate`: structured CI gate config. Use `argv` only; shell-string execution is unsupported.
  **Safety warning:** Disabling the CI gate skips validation before commit/push, which may allow broken code to be pushed.
- `claude_bin`, `codex_bin`, `opencode_bin`, `gemini_bin`, `cursor_bin`, `kimi_bin`, `pi_bin`: override built-in runner executable path/name (Cursor uses the `agent` binary).
- `claude_permission_mode`: `accept_edits` or `bypass_permissions`.
  **Safety warning:** `bypass_permissions` allows Claude to make edits without prompting for approval. Use with caution.
- `runner_cli`: normalized runner CLI behavior (output/approval/sandbox/etc), with global defaults and optional per-runner overrides.
- `instruction_files`: optional list of additional instruction file paths to inject at the top of every prompt sent to runner CLIs (repo-root relative, absolute, or `~/`). Each list entry must be a non-empty path; blank strings are rejected during config validation.

  To inject both global and repo-local AGENTS.md:

  ```json
  {
    "agent": {
      "instruction_files": ["~/.codex/AGENTS.md", "AGENTS.md"]
    }
  }
  ```

Notes:
- Multi-phase runs (`phases >= 2`) always refresh task fields (`scope,evidence,plan,notes,tags,depends_on`) at the start of Phase 1, then generate the plan in that same Phase 1 runner session. This behavior is built in and not configurable.
- `followup_reasoning_effort` is used by Codex and Pi runners and ignored by runners without reasoning-effort support.
- Breaking change: `reasoning_effort` no longer accepts `minimal`; use `low`, `medium`, `high`, or `xhigh`.
- Breaking change in `0.3`: config files must use `"version": 2`, `agent.git_publish_mode`, and the built-in reserved profiles `safe` / `power-user`. `git_commit_push_enabled` is removed. Run `ralph migrate --apply` to rewrite legacy configs before retrying app/CLI commands.
- `make install` updates the CLI and macOS app bundle, but it does not mutate repo-local config files. Older repos still need `ralph migrate --apply` after upgrading to `0.3`.
- CI gate auto-retry: When enabled, Ralph automatically sends a strict compliance message and retries up to 2 times on CI failure during Phase 2, Phase 3, or single-phase execution. This behavior is not configurable; after 2 automatic retries, the user is prompted via the configured `git_revert_mode`. Post-run supervision prompts immediately on CI failure.
- Phase 1 plan-only violations: when `git_revert_mode=ask`, the prompt includes a keep+continue override to proceed to the next phase without reverting changes.
- **Runner session handling**: For runners that support session resumption (e.g., Kimi), Ralph generates unique session IDs per phase (format: `{task_id}-p{phase}-{timestamp}`) and uses explicit `--session` flags rather than runner-specific continue mechanisms. This provides deterministic session management and reliable crash recovery.
- **macOS app boundary**: app-launched runs are noninteractive. The app can display the resolved approval posture, but interactive approvals remain terminal-only until the transport changes.

### `agent.runner_cli`

`agent.runner_cli` provides a normalized configuration surface for runner CLI behavior so Ralph can keep parity across runners while still emitting runner-specific flags.

Structure:
- `agent.runner_cli.defaults`: applied to all runners (unless overridden)
- `agent.runner_cli.runners.<runner>`: per-runner overrides (merged leaf-wise over `defaults`)

Supported normalized fields:
- `output_format`: `stream_json`, `json`, `text` (execution requires `stream_json`)
- `verbosity`: `quiet`, `normal`, `verbose`
- `approval_mode`: `default`, `auto_edits`, `yolo`, `safe`
  **Safety warning:** `yolo` mode bypasses all approval prompts, allowing the runner to make changes without confirmation. The recommended default profile is `safe`.
  
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
  "version": 2,
  "agent": {
    "runner": "codex",
    "model": "gpt-5.4",
    "phases": 3,
    "iterations": 2,
    "reasoning_effort": "high",
    "followup_reasoning_effort": "low",
    "repoprompt_plan_required": false,
    "repoprompt_tool_injection": false,
    "git_publish_mode": "off",
    "git_revert_mode": "ask",
    "claude_permission_mode": "accept_edits",
    "runner_cli": {
      "defaults": {
        "output_format": "stream_json",
        "approval_mode": "default",
        "unsupported_option_policy": "warn"
      },
      "runners": {
        "codex": { "sandbox": "disabled" },
        "claude": { "verbosity": "verbose" }
      }
    },
    "ci_gate": {
      "enabled": true,
      "argv": ["make", "ci"]
    }
  }
}
```

To disable CI gating entirely (skip running any command), set:

```json
{
  "agent": {
    "ci_gate": {
      "enabled": false
    }
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

### `agent.runner_retry`

Runner invocation retry/backoff configuration for transient failure handling. Controls automatic retry behavior when runner invocations fail with transient errors (rate limits, temporary unavailability, network issues). Distinct from webhook retry settings (`agent.webhook.retry_*`).

**Fields:**
- `max_attempts`: Total attempts including initial (default: `3`, range: `1-20`).
- `base_backoff_ms`: Base backoff in milliseconds (default: `1000`, range: `0-600000`).
- `multiplier`: Exponential multiplier (default: `2.0`, range: `1.0-10.0`).
- `max_backoff_ms`: Maximum backoff cap in milliseconds (default: `30000`, range: `0-600000`).
- `jitter_ratio`: Jitter ratio in `[0,1]` for variance (default: `0.2`, range: `0.0-1.0`).

**Retry classification:**
- **Retryable**: Rate limits (HTTP 429), temporary unavailability (HTTP 503), transient I/O errors (connection reset, timeout), and timeouts.
- **Requires user input**: Authentication failures (HTTP 401), missing binaries.
- **Non-retryable**: Invalid invocations, fatal exits, interruptions (Ctrl+C).

**Example:**

```json
{
  "agent": {
    "runner_retry": {
      "max_attempts": 5,
      "base_backoff_ms": 2000,
      "multiplier": 2.0,
      "max_backoff_ms": 60000,
      "jitter_ratio": 0.2
    }
  }
}
```

Notes:
- Retries only occur when the repository is clean (or dirty only in Ralph-allowed paths like `.ralph/`), or when `git_revert_mode` is `enabled` for auto-revert.
- Retry attempt counts and backoff delays are emitted via `RALPH_OPERATION:` markers in runner output.
- To disable retry entirely, set `max_attempts: 1`.

## Profiles

Ralph always exposes two built-in profiles:

- `safe`: recommended default. Uses safer approval defaults and `git_publish_mode = "off"`.
- `power-user`: preserves the higher-autonomy path with `approval_mode = "yolo"` and `git_publish_mode = "commit_and_push"`.

You can inspect resolved profiles with:

```bash
ralph config profiles
```

User-defined profiles remain additive. `safe` and `power-user` are reserved names in `0.3`; defining either in config is a validation error.

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
    "model": "gpt-5.4",
    "reasoning_effort": "medium",
    "phase_overrides": {
      "phase1": {
        "model": "gpt-5.3",
        "reasoning_effort": "high"
      },
      "phase2": {
        "runner": "codex",
        "model": "gpt-5.4",
        "reasoning_effort": "medium"
      },
      "phase3": {
        "runner": "codex",
        "model": "gpt-5.4",
        "reasoning_effort": "high"
      }
    }
  }
}
```

**Precedence (per phase):** CLI phase flags > task phase override (`task.agent.phase_overrides.phaseN.*`) > config phase override (`agent.phase_overrides.phaseN.*`) > CLI global overrides > task global overrides (`task.agent.*`) > config defaults (`agent.*`) > code defaults

## Parallel Configuration

`parallel` controls parallel execution for `ralph run loop` and RalphMac Run Control loop launches.

Key fields:
- `workers`: number of concurrent workers (must be `>= 2`). Default: `null` (disabled unless CLI
  `--parallel` is used).
- `max_push_attempts`: maximum integration loop attempts before giving up (default: `50`).
- `push_backoff_ms`: array of retry backoff intervals in milliseconds (default: `[500, 2000, 5000, 10000]`).
- `workspace_retention_hours`: hours to retain worker workspaces after completion (default: `24`).
- `workspace_root`: root directory for parallel workspaces (default: `<repo-parent>/.workspaces/<repo-name>/parallel`).

  **Git hygiene warning:** If you set `parallel.workspace_root` to a path **inside** the repository (for example `.ralph/workspaces`), you MUST gitignore it (or add it to `.git/info/exclude`). Otherwise Ralph will create workspace clone directories that appear as untracked files and the repo will look "dirty" across runs. Parallel mode will fail fast if the workspace root is inside the repo and not ignored.

Notes:
- CLI flag `--parallel` overrides `parallel.workers` for a single run.
- Workers push directly to the target branch; no PRs are created.
- Use `ralph run parallel status` to check worker states.
- Use `ralph run parallel retry --task <ID>` to retry blocked workers.
- **Breaking change (2026-02):** Parallel mode has been rewritten for direct-push.
  The following config keys have been removed: `auto_pr`, `auto_merge`, `merge_when`,
  `merge_method`, `merge_retries`, `draft_on_failure`, `conflict_policy`, `branch_prefix`,
  `delete_branch_on_merge`, `merge_runner`.
- **Breaking change (2026-02):** The `parallel.worktree_root` config key has been renamed to
  `parallel.workspace_root`. Config files using the old key will fail to load. Run
  `ralph migrate` to update existing configs.

Example:

```json
{
  "parallel": {
    "workers": 3,
    "max_push_attempts": 50,
    "push_backoff_ms": [500, 2000, 5000, 10000],
    "workspace_retention_hours": 24
  }
}
```

## Queue Configuration
`queue` controls file locations, task ID formatting, and auto-archive behavior.

Supported fields:
- `file`: path to the queue file (default: `.ralph/queue.jsonc`).
- `done_file`: path to the done archive (default: `.ralph/done.jsonc`).
- `id_prefix`: task ID prefix (default: `RQ`).
- `id_width`: zero padding width (default: `4`, e.g. `RQ-0001`).
- `auto_archive_terminal_after_days`: automatically archive terminal tasks (done/rejected) from queue to done after this many days (default: `null`/`None`, disabled).

**Parallel mode restriction:** When running `ralph run loop --parallel ...`, `queue.file` and
`queue.done_file` must resolve to paths **under the repository root**. Parallel mode maps these
paths into per-worker workspace clones; paths outside the repo root cannot be mapped safely and are
rejected during parallel preflight. Prefer repo-relative paths like `.ralph/queue.jsonc` and
`.ralph/done.jsonc`.

### Auto-Archive Configuration

The `auto_archive_terminal_after_days` setting provides a queue-level sweep that archives terminal tasks (done/rejected) automatically:

- **Not set / `null`** (default): Disabled; no automatic sweep occurs.
- **`0`**: Archive immediately when the sweep runs (during macOS app startup/reload and after CLI task edit).
- **`N > 0`**: Archive only when `completed_at` is at least `N` days old.

**Safety behavior:** When `N > 0`, tasks with missing, blank, or invalid `completed_at` timestamps are **not moved**. This ensures only tasks with valid completion timestamps are archived automatically.

Example configurations:

```json
{
  "version": 2,
  "queue": {
    "file": ".ralph/queue.jsonc",
    "done_file": ".ralph/done.jsonc",
    "id_prefix": "RQ",
    "id_width": 4,
    "auto_archive_terminal_after_days": 7
  }
}
```

Immediate archive (archive all terminal tasks on sweep):
```json
{
  "queue": {
    "auto_archive_terminal_after_days": 0
  }
}
```

The queue-level sweep runs:
- When the macOS app starts or reloads queue files
- After `ralph task edit` operations (CLI)

For immediate manual archiving, use `ralph queue archive`.

### Aging Thresholds

`queue.aging_thresholds` controls the day thresholds for `ralph queue aging` task categorization.
This helps identify stale work by grouping tasks into buckets based on their age.

Supported fields:
- `warning_days`: warn when age is strictly greater than N days (default: `7`)
- `stale_days`: stale when age is strictly greater than N days (default: `14`)
- `rotten_days`: rotten when age is strictly greater than N days (default: `30`)

**Ordering invariant:** Config validation enforces `warning_days < stale_days < rotten_days`.

**Age computation by status:**
- `draft`, `todo`: uses `created_at` timestamp
- `doing`: uses `started_at` if present, otherwise `created_at`
- `done`, `rejected`: uses `completed_at` if present, then `updated_at`, then `created_at`

Tasks with missing/invalid timestamps or future timestamps are categorized as `unknown`.

Example configuration:
```json
{
  "version": 2,
  "queue": {
    "aging_thresholds": {
      "warning_days": 5,
      "stale_days": 10,
      "rotten_days": 20
    }
  }
}
```

## Precedence
1. CLI flags (single run)
2. Project config (`.ralph/config.jsonc`)
3. Global config (`~/.config/ralph/config.jsonc`)
4. Schema defaults (`schemas/config.schema.json`)

## App Safety Warnings (macOS)

When editing configuration in the macOS app, certain high-risk settings display inline warnings:

- **Danger level** (⚠): Settings like `git_publish_mode` that can cause irreversible actions. The app prompts for confirmation before enabling these.
- **Warning level** (ℹ): Settings like `approval_mode` and `claude_permission_mode` that reduce safety checks. These show descriptive text but don't require confirmation.

The confirmation dialog for Danger-level settings explains the risk and requires an explicit confirmation to proceed.

## Notification Configuration

`agent.notification` controls desktop notifications for task completion and failures.

Supported fields:
- `enabled`: legacy field, enable/disable all notifications (default: `true`).
- `notify_on_complete`: enable notifications on task completion (default: `true`).
- `notify_on_fail`: enable notifications on task failure (default: `true`).
- `notify_on_loop_complete`: enable notifications when loop mode finishes (default: `true`).
- `notify_on_watch_new_tasks`: enable notifications when watch mode adds new tasks from comments (default: `true`).
- `suppress_when_active`: suppress notifications when the macOS app is active (default: `true`).
- `sound_enabled`: play sound with notification (default: `false`).
- `sound_path`: custom sound file path (optional, platform-specific).
- `timeout_ms`: notification display duration in milliseconds (default: `8000`, range: `1000-60000`).

Platform notes:
- **macOS**: Uses NotificationCenter; sound plays via `afplay` (default: `/System/Library/Sounds/Glass.aiff`).
- **Linux**: Uses D-Bus/notify-send; sound plays via `paplay`/`aplay` or `canberra-gtk-play` for default sounds.

## Webhook Configuration

![Webhooks](assets/images/2026-02-07-webhooks.png)

`agent.webhook` controls HTTP webhook notifications for task events. Webhooks complement desktop notifications by enabling external integrations (Slack, Discord, CI systems, etc.) to receive real-time task events.

Supported fields:
- `enabled`: enable webhook notifications (default: `false`).
- `url`: webhook endpoint URL (required when enabled).
- `allow_insecure_http`: when `true`, allow `http://` URLs (default: `false` / unset; HTTPS-only).
- `allow_private_targets`: when `true`, allow loopback, link-local, and common cloud-metadata hostnames/IPs (default: `false` / unset).
- `secret`: secret key for HMAC-SHA256 signature generation (optional).
  When set, webhooks include an `X-Ralph-Signature` header for verification.
- `events`: list of events to subscribe to (default: legacy task events only).
  - **Task events**: `task_created`, `task_started`, `task_completed`, `task_failed`, `task_status_changed`
  - **Loop events**: `loop_started`, `loop_stopped` (opt-in)
  - **Phase events**: `phase_started`, `phase_completed` (opt-in)
  - **Queue event**: `queue_unblocked` (opt-in)
  - Use `["*"]` to subscribe to all events including new ones
- `timeout_secs`: request timeout in seconds (default: `30`, max: `300`).
- `retry_count`: number of retry attempts for failed deliveries (default: `3`, max: `10`).
- `retry_backoff_ms`: base interval in milliseconds for exponential webhook retry delays (default: `1000`, max: `30000`); delays include bounded jitter and cap at 30 seconds between attempts.
- `queue_capacity`: maximum number of pending webhooks in the delivery queue (default: `500`, range: `10-10000`).
- `parallel_queue_multiplier`: multiplier for effective queue capacity in parallel mode (default: `2.0`, range: `1.0-10.0`).
- `queue_policy`: backpressure policy when queue is full (default: `drop_oldest`).
  - `drop_oldest`: Drop new webhooks when queue is full (preserves existing queue contents).
  - `drop_new`: Drop the new webhook if the queue is full.
  - `block_with_timeout`: Briefly block the caller (100ms), then drop if queue is still full.

URL validation (when `enabled` is `true`): only `http://` and `https://` are accepted; `http://` requires `allow_insecure_http: true`. Loopback, IPv4 link-local (`169.254.0.0/16`), IPv6 link-local, unspecified addresses, and `metadata.google.internal` are rejected unless `allow_private_targets: true`.

### Event Filtering

**Breaking change**: As of this version, new event types (`loop_*`, `phase_*`) are **opt-in** and not enabled by default.

- If `events` is not specified (or `null`): only legacy task events are delivered
- If `events` is `["*"]`: all events are delivered (legacy + new)
- If `events` is an explicit list: only those events are delivered

Example configuration for CI/dashboard integrations:

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

### Delivery Semantics

Webhooks are delivered **asynchronously** by a background worker thread:

- **Best-effort delivery**: Webhooks may be dropped if the queue is full (per `queue_policy`).
- **Non-blocking**: The `send_webhook` call returns immediately after enqueueing.
- **Order preservation**: Webhooks are delivered in FIFO order within the constraints of the backpressure policy.
- **Failure handling**: Failed deliveries are retried (per `retry_count` and `retry_backoff_ms`).
- **Failure persistence**: Final delivery failures are persisted to `.ralph/cache/webhooks/failures.json` (bounded to the most recent 200 records).
- **Worker lifecycle**: The background worker starts on first webhook send and shuts down when the process exits.

Example:

```json
{
  "version": 2,
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

**Task events** (`task_*`) always include `task_id` and `task_title`. **Loop events** (`loop_*`) omit these fields since they are not task-specific.

#### Enriched Payloads (Phase Events)

Phase and loop events include additional context metadata:

```json
{
  "event": "phase_completed",
  "timestamp": "2024-01-15T10:30:00Z",
  "task_id": "RQ-0001",
  "task_title": "Add webhook support",
  "runner": "codex",
  "model": "gpt-5.4",
  "phase": 2,
  "phase_count": 3,
  "duration_ms": 12500,
  "repo_root": "/home/user/project",
  "branch": "main",
  "commit": "abc123def456",
  "ci_gate": "passed"
}
```

Optional context fields (only present when applicable):
- `runner`: The runner used (e.g., `claude`, `codex`, `kimi`)
- `model`: The model used for this phase
- `phase`: Phase number (1, 2, or 3)
- `phase_count`: Total configured phases
- `duration_ms`: Phase execution duration in milliseconds
- `repo_root`: Repository root path
- `branch`: Current git branch
- `commit`: Current git commit hash
- `ci_gate`: CI gate outcome (`skipped`, `passed`, or `failed`)

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

# Test with new event types (opt-in)
ralph webhook test --event phase_started
ralph webhook test --event loop_started

# Print the JSON payload without sending (useful for debugging)
ralph webhook test --event phase_completed --print-json
ralph webhook test --event task_created --print-json --pretty

# Test with custom URL
ralph webhook test --url https://example.com/webhook

# Inspect queue/failure diagnostics
ralph webhook status
ralph webhook status --format json

# Replay failed deliveries safely
ralph webhook replay --dry-run --id wf-1700000000-1
ralph webhook replay --event task_completed --limit 5
```

Non-dry-run replay (`ralph webhook replay` without `--dry-run`) requires:
- `agent.webhook.enabled: true`
- `agent.webhook.url` set to a non-empty endpoint URL

- **Windows**: Uses toast notifications; custom sounds are `.wav`-only and play via `winmm.dll` PlaySound.

Example:

```json
{
  "version": 2,
  "agent": {
    "notification": {
      "enabled": true,
      "notify_on_complete": true,
      "notify_on_fail": true,
      "notify_on_loop_complete": true,
      "notify_on_watch_new_tasks": true,
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

Project-local plugin settings and project-scope plugin directories require repo trust (see [Repo execution trust](#repo-execution-trust)). In untrusted repos, Ralph ignores `.ralph/plugins/*` during runtime discovery.

Supported fields:
- `plugins.plugins.<id>.enabled`: enable/disable the plugin (default: `false`).
- `plugins.plugins.<id>.config`: opaque configuration blob passed to the plugin.

Plugin directories (searched in order, project overrides global):
- Project: `.ralph/plugins/<plugin_id>/plugin.json`
- Global: `~/.config/ralph/plugins/<plugin_id>/plugin.json`

Example:

```json
{
  "version": 2,
  "plugins": {
    "plugins": {
      "my.custom-runner": {
        "enabled": true,
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

## Profiles

Configuration profiles enable quick switching between different workflow presets without manually editing config or passing many CLI flags for each invocation.

A profile is an `AgentConfig`-shaped patch that is applied over the base `agent` configuration when selected via `--profile <NAME>`.

Define custom profiles in your config file under the `profiles` key:

```json
{
  "version": 2,
  "profiles": {
    "fast-local": {
      "runner": "codex",
      "model": "gpt-5.4",
      "phases": 1,
      "reasoning_effort": "low"
    },
    "deep-review": {
      "runner": "codex",
      "model": "gpt-5.4",
      "phases": 3,
      "reasoning_effort": "high"
    }
  }
}
```

### Profile Precedence

When a profile is selected, the final configuration is computed in this order (highest to lowest):

1. **CLI flags** (e.g., `--runner`, `--model`, `--phases`, `--effort`)
2. **Task overrides** (`task.agent.*` in the queue)
3. **Selected profile** (config-defined)
4. **Base config** (merged global + project config)

This means:
- CLI flags always win
- A profile can be partially overridden by CLI flags

### Using Profiles

Select a profile using the `--profile` flag:

```bash
# Run with a custom fast-local profile
ralph run one --profile fast-local

# Scan with a deep-review profile
ralph scan --profile deep-review "security audit"

# Override specific settings while using a profile
ralph run one --profile fast-local --phases 2 --runner claude

# List available profiles
ralph config profiles list

# Inspect a specific profile
ralph config profiles show fast-local
```

### Profile Inheritance

Profiles are merged into the base config using the same leaf-wise merge semantics as config layers:

- `Some(value)` in the profile overrides the base config
- `None` or omitted fields inherit from the base config

This means a profile only needs to specify the fields it wants to change:

```json
{
  "profiles": {
    "fast-local": {
      "phases": 1
    }
  }
}
```

The above profile only changes `phases`, leaving all other `agent` settings at their base values.

### Migration from Retired Default Names

`quick` and `thorough` are no longer built in. If you want those names back for your team, define them explicitly:

```json
{
  "profiles": {
    "quick": {
      "runner": "codex",
      "model": "gpt-5.4",
      "phases": 1,
      "reasoning_effort": "low"
    },
    "thorough": {
      "runner": "codex",
      "model": "gpt-5.4",
      "phases": 3,
      "reasoning_effort": "high"
    }
  }
}
```
