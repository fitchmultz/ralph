# Configuration

Purpose: Document Ralph's JSON configuration layout, defaults, and override precedence for global and project settings.

## Overview
Ralph reads JSON configuration from two locations, with project config taking precedence over global:
- Global: `~/.config/ralph/config.json`
- Project: `.ralph/config.json`

CLI flags override both for a single run. Defaults are defined by `schemas/config.schema.json`.

## Top-Level Fields
- `version` (number): Config schema version. Default: `1`.
- `project_type` (string or null): `code` or `docs`. Default: `code`.
- `agent` (object): Runner defaults (CLI binaries, runner, model, phases, and prompt enforcement).
- `queue` (object): Queue file locations and task ID formatting.

## Agent Configuration
`agent` controls default execution settings. Defaults are schema-defined.

Supported fields:
- `runner`: `codex`, `opencode`, `gemini`, or `claude`.
- `model`: default model id (string).
- `phases`: number of phases (1, 2, or 3).
- `reasoning_effort`: `minimal`, `low`, `medium`, `high` (Codex only).
- `require_repoprompt`: `true` or `false`.
- `git_revert_mode`: `ask`, `enabled`, or `disabled`.
- `claude_bin`, `codex_bin`, `opencode_bin`, `gemini_bin`: override runner executable path/name.
- `claude_permission_mode`: `accept_edits` or `bypass_permissions`.

Example:
```json
{
  "version": 1,
  "agent": {
    "runner": "claude",
    "model": "sonnet",
    "phases": 3,
    "require_repoprompt": false,
    "git_revert_mode": "ask",
    "claude_permission_mode": "bypass_permissions"
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
