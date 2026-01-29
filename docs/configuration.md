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
- `queue` (object): Queue file locations and task ID formatting.

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
- `ci_gate_command`: command to run for the CI gate (default: `make ci`).
- `ci_gate_enabled`: enable or disable the CI gate (default: `true`).
  **Safety warning:** Disabling the CI gate skips validation before commit/push, which may allow broken code to be pushed.
- `claude_bin`, `codex_bin`, `opencode_bin`, `gemini_bin`, `cursor_bin`: override runner executable path/name (Cursor uses the `agent` binary).
- `claude_permission_mode`: `accept_edits` or `bypass_permissions`.
  **Safety warning:** `bypass_permissions` allows Claude to make edits without prompting for approval. Use with caution.
- `runner_cli`: normalized runner CLI behavior (output/approval/sandbox/etc), with global defaults and optional per-runner overrides.
- `instruction_files`: optional list of additional instruction file paths to inject at the top of every prompt sent to runner CLIs (repo-root relative, absolute, or `~/`).

Notes:
- `followup_reasoning_effort` is ignored for non-Codex runners.
- Breaking change: `reasoning_effort` no longer accepts `minimal`; use `low`, `medium`, `high`, or `xhigh`.
- CI gate auto-retry: When enabled, Ralph automatically sends a strict compliance message and retries up to 2 times on CI failure during Phase 2, Phase 3, or single-phase execution. This behavior is not configurable; after 2 automatic retries, the user is prompted via the configured `git_revert_mode`. Post-run supervision prompts immediately on CI failure.
- Phase 1 plan-only violations: when `git_revert_mode=ask`, the prompt includes a keep+continue override to proceed to the next phase without reverting changes.

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
- `sandbox`: `default`, `enabled`, `disabled`
- `plan_mode`: `default`, `enabled`, `disabled`
- `unsupported_option_policy`: `ignore`, `warn`, `error`

Notes:
- Unsupported options are dropped by default with a warning (policy `warn`).
- `agent.claude_permission_mode` remains supported; when `runner_cli.approval_mode` is set, it takes precedence for Claude mapping.
- `AGENTS.md` at the repo root is injected automatically when present.

Example:
```json
{
  "version": 1,
  "agent": {
    "runner": "codex",
    "model": "gpt-5.2-codex",
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

## Queue Configuration
`queue` controls file locations and task ID formatting.

Supported fields:
- `file`: path to the queue file (default: `.ralph/queue.json`).
- `done_file`: path to the done archive (default: `.ralph/done.json`).
- `id_prefix`: task ID prefix (default: `RQ`).
- `id_width`: zero padding width (default: `4`, e.g. `RQ-0001`).

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

`agent.notification` controls desktop notifications when tasks complete successfully.

Supported fields:
- `enabled`: enable/disable notifications (default: `true`).
- `sound_enabled`: play sound with notification (default: `false`).
- `sound_path`: custom sound file path (optional, platform-specific).
- `timeout_ms`: notification display duration in milliseconds (default: `8000`, range: `1000-60000`).

Platform notes:
- **macOS**: Uses NotificationCenter; sound plays via `afplay` (default: `/System/Library/Sounds/Glass.aiff`).
- **Linux**: Uses D-Bus/notify-send; sound plays via `paplay`/`aplay` or `canberra-gtk-play` for default sounds.
- **Windows**: Uses toast notifications; custom sounds play via `winmm.dll` PlaySound for `.wav` files, PowerShell MediaPlayer fallback for other formats.

Example:
```json
{
  "version": 1,
  "agent": {
    "notification": {
      "enabled": true,
      "sound_enabled": true,
      "timeout_ms": 10000
    }
  }
}
```

CLI overrides:
- `--notify`: Enable notification for this run (overrides config).
- `--no-notify`: Disable notification for this run (overrides config).
- `--notify-sound`: Enable sound for this run (works with `--notify` or when enabled in config).
