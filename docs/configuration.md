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
- `reasoning_effort`: `low`, `medium`, `high`, `xhigh` (Codex only).
- `iterations`: number of iterations to run per task (default: 1).
- `followup_reasoning_effort`: reasoning effort for iterations after the first (Codex only).
- `repoprompt_plan_required`: require RepoPrompt planning instructions (context_builder) during Phase 1.
- `repoprompt_tool_injection`: inject RepoPrompt tooling reminders into prompts.
- `require_repoprompt`: `true` or `false` (deprecated; maps to both `repoprompt_plan_required` and `repoprompt_tool_injection` when the new fields are unset).
- `git_revert_mode`: `ask`, `enabled`, or `disabled`.
- `git_commit_push_enabled`: enable or disable automatic git commit/push after successful runs (default: `true`).
- `ci_gate_command`: command to run for the CI gate (default: `make ci`).
- `ci_gate_enabled`: enable or disable the CI gate (default: `true`).
- `claude_bin`, `codex_bin`, `opencode_bin`, `gemini_bin`: override runner executable path/name.
- `claude_permission_mode`: `accept_edits` or `bypass_permissions`.

Notes:
- `followup_reasoning_effort` is ignored for non-Codex runners.
- Breaking change: `reasoning_effort` no longer accepts `minimal`; use `low`, `medium`, `high`, or `xhigh`.
- CI gate auto-retry: When enabled, Ralph automatically sends a strict compliance message and retries up to 2 times on CI failure during Phase 2, Phase 3, or single-phase execution. This behavior is not configurable; after 2 automatic retries, the user is prompted via the configured `git_revert_mode`. Post-run supervision prompts immediately on CI failure.
- Phase 1 plan-only violations: when `git_revert_mode=ask`, the prompt includes a keep+continue override to proceed to the next phase without reverting changes.

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
