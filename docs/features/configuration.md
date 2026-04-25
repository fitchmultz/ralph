# Ralph Configuration System
Status: Active
Owner: Maintainers
Source of truth: this document for its stated scope
Parent: [Feature Documentation](README.md)


Ralph uses a flexible, layered JSON configuration system that allows you to customize behavior at both global (user) and project levels. Configuration supports JSON with Comments (JSONC) for better documentation and organization.

---

## Table of Contents

1. [Overview](#overview)
2. [Config Locations](#config-locations)
3. [Precedence](#precedence)
4. [JSONC Support](#jsonc-support)
5. [Top-Level Fields](#top-level-fields)
6. [Agent Configuration](#agent-configuration)
7. [Parallel Configuration](#parallel-configuration)
8. [Queue Configuration](#queue-configuration)
9. [Plugin Configuration](#plugin-configuration)
10. [Profile Configuration](#profile-configuration)
11. [Environment Variables](#environment-variables)
12. [Validation](#validation)

---

## Overview

Ralph's configuration system uses a two-layer architecture:

- **Global Config**: User-wide defaults stored in your home directory
- **Project Config**: Repository-specific overrides stored in `.ralph/config.jsonc`

Configuration is merged using **leaf-wise semantics**: when a value is present (`Some`), it overrides; when absent (`None`), it inherits from the parent layer. This allows fine-grained control over which settings to override.

### Configuration Flow

```
CLI Flags → Project Config → Global Config → Schema Defaults
     ↑                                                        
     └────────────────── Profiles ───────────────────────────┘
```

---

## Config Locations

### Global Config

Stored in your user configuration directory:

| Platform | Path |
|----------|------|
| Linux/macOS | `~/.config/ralph/config.jsonc` |
| With XDG | `$XDG_CONFIG_HOME/ralph/config.jsonc` |

Create this file manually. A minimal example:

```json
{
  "version": 2,
  "agent": {
    "runner": "codex",
    "model": "gpt-5.4",
    "phases": 3
  }
}
```

You can also use `.jsonc` extension for JSON with Comments support.

### Project Config

Stored within each repository:

```
<repo-root>/.ralph/config.jsonc
```

Created automatically when you run `ralph init` in a repository.

### Quick Reference

```bash
# View resolved configuration
ralph config show

# View in JSON format
ralph config show --format json

# View config file paths
ralph config paths

# View configuration schema
ralph config schema

# List available profiles
ralph config profiles list
```

---

## Precedence

Configuration values are resolved in the following order (highest to lowest):

| Priority | Source | Description |
|----------|--------|-------------|
| 1 | **CLI Flags** | Command-line arguments (`--runner`, `--model`, etc.) |
| 2 | **Task Overrides** | Per-task settings in `task.agent.*` |
| 3 | **Profiles** | Selected profile configuration |
| 4 | **Project Config** | `.ralph/config.jsonc` |
| 5 | **Global Config** | `~/.config/ralph/config.jsonc` |
| 6 | **Schema Defaults** | Built-in defaults from `schemas/config.schema.json` |

### Example Precedence

Given:
- **Global config**: `agent.runner = "claude"`
- **Project config**: `agent.runner = "codex"`
- **CLI flag**: `--runner kimi`

Result: `kimi` wins (CLI flag > project config > global config).

### Profile Precedence

When using profiles, the order is:

1. CLI flags
2. Task overrides (`task.agent.*`)
3. Selected profile
4. Base config (global + project merged)

---

## JSONC Support

Ralph supports **JSON with Comments** (JSONC) for all configuration and queue files. This allows you to document your configuration directly in the files.

### Supported Comment Styles

```jsonc
{
  // Single-line comment
  "version": 2,
  
  /* 
   * Multi-line comment
   * Great for longer explanations
   */
  "agent": {
    "runner": "claude",  // Trailing comments work too
    "phases": 3,
  },  // Trailing commas are also supported!
}
```

### File Extensions

- `.jsonc` - Runtime config and queue files
- `.json` - Strict JSON where external tools require it

Ralph preserves JSONC support for runtime-edited config and queue files. Comments are not preserved when Ralph rewrites those files.

### Best Practices

```jsonc
{
  // Schema version - must be 2
  "version": 2,
  
  // Project type affects prompt defaults
  // Options: "code" | "docs"
  "project_type": "code",
  
  "agent": {
    /* Runner selection:
       - claude: Anthropic Claude Code
       - codex: OpenAI Codex CLI
       - gemini: Google Gemini CLI
       - opencode: OpenCode
       - cursor: Cursor agent
       - kimi: Kimi Code CLI */
    "runner": "claude",
    
    // Execution phases: 1 (single-pass), 2 (plan+implement), 3 (plan+implement+review)
    "phases": 3
  }
}
```

---

## Top-Level Fields

The configuration root contains these main sections:

| Field | Type | Description |
|-------|------|-------------|
| `version` | `number` | Config schema version (must be `1`) |
| `project_type` | `"code" \| "docs"` | Drives prompt defaults and workflow decisions |
| `agent` | `AgentConfig` | Runner defaults and execution settings |
| `parallel` | `ParallelConfig` | Parallel execution configuration |
| `queue` | `QueueConfig` | Queue file locations and ID formatting |
| `plugins` | `PluginsConfig` | Plugin enable/disable and settings |
| `profiles` | `object` | Named configuration profiles |

### Example Structure

```json
{
  "version": 2,
  "project_type": "code",
  "agent": { /* ... */ },
  "parallel": { /* ... */ },
  "queue": { /* ... */ },
  "plugins": { /* ... */ },
  "profiles": { /* ... */ }
}
```

---

## Agent Configuration

The `agent` section controls how Ralph executes tasks and interacts with AI runners.

### Core Fields

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `runner` | `string` | `"codex"` | Default runner: `claude`, `codex`, `gemini`, `opencode`, `cursor`, `kimi`, `pi` |
| `model` | `string` | `"gpt-5.4"` | Default model identifier |
| `phases` | `1 \| 2 \| 3` | `3` | Execution phases: 1=single-pass, 2=plan+implement, 3=plan+implement+review |
| `iterations` | `number` | `1` | Number of iterations per task (≥1) |
| `reasoning_effort` | `"low" \| "medium" \| "high" \| "xhigh"` | `"medium"` | Reasoning depth (Codex and Pi only) |
| `followup_reasoning_effort` | `"low" \| "medium" \| "high" \| "xhigh"` | `null` | Reasoning for iterations > 1 |

### Runner Binary Paths

Override executable names/paths for each runner:

| Field | Default | Description |
|-------|---------|-------------|
| `claude_bin` | `"claude"` | Claude Code executable |
| `codex_bin` | `"codex"` | OpenAI Codex CLI |
| `gemini_bin` | `"gemini"` | Google Gemini CLI |
| `opencode_bin` | `"opencode"` | OpenCode CLI |
| `cursor_bin` | `"agent"` | Cursor agent (note: uses `agent`, not `cursor`) |
| `kimi_bin` | `"kimi"` | Kimi Code CLI |
| `pi_bin` | `"pi"` | Pi CLI |

### Permission & Safety

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `claude_permission_mode` | `"accept_edits" \| "bypass_permissions"` | `"accept_edits"` | Claude permission handling |
| `git_revert_mode` | `"ask" \| "enabled" \| "disabled"` | `"ask"` | Auto-revert behavior on errors |
| `git_publish_mode` | `"off" \| "commit" \| "commit_and_push"` | `"off"` | Post-run git publication mode |

> ⚠️ **Safety Warning**: `bypass_permissions` allows Claude to make edits without prompting. The default-safe path uses `accept_edits`.

### CI Gate

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `ci_gate.enabled` | `boolean` | `true` | Enable CI gate validation |
| `ci_gate.argv` | `string[]` | `["make", "ci"]` | Direct argv command to run for the CI gate |

> ⚠️ **Safety Warning**: Disabling the CI gate skips validation before commit/push, potentially allowing broken code.

### RepoPrompt Integration

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `repoprompt_plan_required` | `boolean` | `false` | Require RepoPrompt during planning |
| `repoprompt_tool_injection` | `boolean` | `false` | Inject RepoPrompt tooling reminders |

### Instruction Files

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `instruction_files` | `string[]` | `null` | Additional instruction files to inject into prompts |

Each list entry must be a non-empty path; blank strings are rejected during config validation.

Paths can be:
- Absolute: `/path/to/instructions.md`
- Home-relative: `~/.codex/AGENTS.md`
- Repo-relative: `AGENTS.md`

```json
{
  "agent": {
    "instruction_files": [
      "~/.config/ralph/global-agents.md",
      "AGENTS.md",
      "docs/project-guidelines.md"
    ]
  }
}
```

### Session & Recovery

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `session_timeout_hours` | `number` | `24` | Session timeout for crash recovery (≥1) |

### Scan Prompts

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `scan_prompt_version` | `"v1" \| "v2"` | `"v2"` | Scan prompt version: v1 (rule-based), v2 (rubric-based) |

### Complete Agent Example

```json
{
  "version": 2,
  "agent": {
    "runner": "claude",
    "model": "sonnet",
    "phases": 3,
    "iterations": 1,
    "reasoning_effort": "high",
    "claude_permission_mode": "accept_edits",
    "git_revert_mode": "ask",
    "git_publish_mode": "commit_and_push",
    "ci_gate": {
      "enabled": true,
      "argv": ["make", "ci"]
    },
    "repoprompt_plan_required": false,
    "instruction_files": ["AGENTS.md"],
    "session_timeout_hours": 24,
    "scan_prompt_version": "v2"
  }
}
```

---

### `agent.runner_cli`

Normalized runner CLI behavior configuration with global defaults and per-runner overrides.

#### Structure

```json
{
  "agent": {
    "runner_cli": {
      "defaults": { /* applied to all runners */ },
      "runners": {
        "claude": { /* per-runner overrides */ },
        "codex": { /* per-runner overrides */ }
      }
    }
  }
}
```

#### Options

| Field | Type | Description |
|-------|------|-------------|
| `output_format` | `"stream_json" \| "json" \| "text"` | Runner output format |
| `verbosity` | `"quiet" \| "normal" \| "verbose"` | Output verbosity |
| `approval_mode` | `"default" \| "auto_edits" \| "yolo" \| "safe"` | Permission behavior |
| `sandbox` | `"default" \| "enabled" \| "disabled"` | Sandbox mode |
| `plan_mode` | `"default" \| "enabled" \| "disabled"` | Plan/read-only behavior |
| `unsupported_option_policy` | `"ignore" \| "warn" \| "error"` | Handle unsupported options |

> ⚠️ **Safety Warning**: `approval_mode: "yolo"` bypasses all approval prompts. Use with extreme caution.

> **Note**: Ralph does NOT pass approval flags to Codex. Configure Codex approval in `~/.codex/config.json`.

#### Example

```json
{
  "agent": {
    "runner_cli": {
      "defaults": {
        "output_format": "stream_json",
        "approval_mode": "yolo",
        "unsupported_option_policy": "warn"
      },
      "runners": {
        "codex": { "sandbox": "disabled" },
        "claude": { "verbosity": "verbose" },
        "kimi": { "approval_mode": "yolo" }
      }
    }
  }
}
```

---

### `agent.phase_overrides`

Per-phase overrides for runner, model, and reasoning effort. Allows different settings for each execution phase.

#### Structure

```json
{
  "agent": {
    "phase_overrides": {
      "phase1": { /* planning phase */ },
      "phase2": { /* implementation phase */ },
      "phase3": { /* review phase */ }
    }
  }
}
```

#### Phase Override Fields

| Field | Type | Description |
|-------|------|-------------|
| `runner` | `string` | Override runner for this phase |
| `model` | `string` | Override model for this phase |
| `reasoning_effort` | `"low" \| "medium" \| "high" \| "xhigh"` | Override reasoning for this phase |

#### Example

```json
{
  "agent": {
    "runner": "codex",
    "model": "gpt-5.4",
    "reasoning_effort": "medium",
    "phase_overrides": {
      "phase1": {
        "model": "gpt-5.4",
        "reasoning_effort": "high"
      },
      "phase2": {
        "runner": "codex",
        "model": "gpt-5.4"
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

---

### `agent.runner_retry`

Configuration for automatic retry of runner invocations on transient failures.

| Field | Type | Default | Range | Description |
|-------|------|---------|-------|-------------|
| `max_attempts` | `number` | `3` | 1-20 | Total attempts (including initial) |
| `base_backoff_ms` | `number` | `1000` | 0-600000 | Initial backoff in milliseconds |
| `multiplier` | `number` | `2.0` | 1.0-10.0 | Exponential backoff multiplier |
| `max_backoff_ms` | `number` | `30000` | 0-600000 | Maximum backoff cap |
| `jitter_ratio` | `number` | `0.2` | 0.0-1.0 | Random jitter ratio |

#### Example

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

---

### `agent.notification`

Desktop notification configuration for task events.

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `enabled` | `boolean` | `true` | Legacy field (prefer `notify_on_complete`) |
| `notify_on_complete` | `boolean` | `true` | Notify on task completion |
| `notify_on_fail` | `boolean` | `true` | Notify on task failure |
| `notify_on_loop_complete` | `boolean` | `true` | Notify when loop mode completes |
| `notify_on_watch_new_tasks` | `boolean` | `true` | Notify when watch mode adds new tasks from comments |
| `suppress_when_active` | `boolean` | `true` | Suppress when the macOS app is active |
| `sound_enabled` | `boolean` | `false` | Play sound with notifications |
| `sound_path` | `string` | `null` | Custom sound file path (`.wav` only on Windows) |
| `timeout_ms` | `number` | `8000` | Notification timeout (1000-60000) |

#### Platform Notes

- **macOS**: Uses NotificationCenter; sound via `afplay`
- **Linux**: Uses D-Bus/notify-send; sound via `paplay`/`aplay`
- **Windows**: Uses toast notifications; sound via `winmm.dll`

---

### `agent.webhook`

HTTP webhook configuration for external integrations.

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `enabled` | `boolean` | `false` | Enable webhooks |
| `url` | `string` | `null` | Webhook endpoint URL |
| `allow_insecure_http` | `boolean` | `false` | Allow `http://` URLs (default HTTPS-only) |
| `allow_private_targets` | `boolean` | `false` | Allow loopback, link-local, and metadata-style hosts |
| `secret` | `string` | `null` | HMAC-SHA256 secret for signatures |
| `events` | `string[]` | `null` | Events to subscribe to (use `["*"]` for all) |
| `timeout_secs` | `number` | `30` | Request timeout (1-300) |
| `retry_count` | `number` | `3` | Retry attempts (0-10) |
| `retry_backoff_ms` | `number` | `1000` | Base for exponential retry delays (100-30000); bounded jitter; 30s cap between attempts |
| `queue_capacity` | `number` | `500` | Delivery queue size (10-10000) |
| `parallel_queue_multiplier` | `number` | `2.0` | Parallel-mode queue capacity multiplier (1.0-10.0) |
| `queue_policy` | `"drop_oldest" \| "drop_new" \| "block_with_timeout"` | `"drop_oldest"` | Backpressure policy |

#### Events

- **Task events**: `task_created`, `task_started`, `task_completed`, `task_failed`, `task_status_changed`
- **Loop events**: `loop_started`, `loop_stopped` (opt-in)
- **Phase events**: `phase_started`, `phase_completed` (opt-in)

#### Example

```json
{
  "agent": {
    "webhook": {
      "enabled": true,
      "url": "https://hooks.slack.com/services/...",
      "secret": "my-webhook-secret",
      "events": ["task_completed", "task_failed"],
      "timeout_secs": 30,
      "retry_count": 3,
      "queue_capacity": 100
    }
  }
}
```

---

## Parallel Configuration

The `parallel` section controls parallel task execution for `ralph run loop` and RalphMac Run Control loop launches.

### Core Fields

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `workers` | `number` | `null` | Concurrent workers (≥2, null = disabled unless `--parallel` is used) |
| `workspace_root` | `string` | `<repo-parent>/.workspaces/<repo-name>/parallel` | Root for parallel worker workspaces |
| `max_push_attempts` | `number` | `50` | Max integration attempts before worker becomes blocked |
| `push_backoff_ms` | `number[]` | `[500, 2000, 5000, 10000]` | Backoff between integration retries |
| `workspace_retention_hours` | `number` | `24` | Hours to retain completed/failed worker workspaces |

Parallel mode uses direct push to the coordinator base branch. Workers run agent-owned integration (`fetch/rebase/conflict-fix/commit/push`) and no PR lifecycle is used.

> ⚠️ **Git Hygiene**: If `workspace_root` is inside the repo, you MUST gitignore it or parallel mode will fail preflight checks.

### Removed Legacy Keys (No Longer Supported)

The following PR-era keys were removed from parallel mode and are invalid in current configs:
- `auto_pr`
- `auto_merge`
- `merge_when`
- `merge_method`
- `merge_retries`
- `draft_on_failure`
- `conflict_policy`
- `branch_prefix`
- `delete_branch_on_merge`
- `merge_runner`

### Complete Parallel Example

```json
{
  "version": 2,
  "parallel": {
    "workers": 3,
    "workspace_root": ".workspaces/my-repo/parallel",
    "max_push_attempts": 50,
    "push_backoff_ms": [500, 2000, 5000, 10000],
    "workspace_retention_hours": 24
  }
}
```

---

## Queue Configuration

The `queue` section controls task queue file locations, ID formatting, and maintenance behavior.

### File Paths

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `file` | `string` | `".ralph/queue.jsonc"` | Path to active queue file |
| `done_file` | `string` | `".ralph/done.jsonc"` | Path to done archive |

Paths are relative to repo root. Absolute paths and `~` expansion are supported.

### ID Formatting

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `id_prefix` | `string` | `"RQ"` | Task ID prefix |
| `id_width` | `number` | `4` | Zero-padding width (e.g., 4 = RQ-0001) |

### Warnings & Limits

| Field | Type | Default | Range | Description |
|-------|------|---------|-------|-------------|
| `size_warning_threshold_kb` | `number` | `500` | 100-10000 | Warn if queue file exceeds this size |
| `task_count_warning_threshold` | `number` | `500` | 50-5000 | Warn if queue has too many tasks |
| `max_dependency_depth` | `number` | `10` | 1-100 | Max dependency chain depth |

### Auto-Archive

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `auto_archive_terminal_after_days` | `number \| null` | `null` | Auto-archive done/rejected tasks after N days |

Semantics:
- `null`: Disabled (default)
- `0`: Archive immediately on sweep
- `N`: Archive when `completed_at` is at least N days old

### Aging Thresholds

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `aging_thresholds.warning_days` | `number` | `7` | Warn when task age > N days |
| `aging_thresholds.stale_days` | `number` | `14` | Stale when task age > N days |
| `aging_thresholds.rotten_days` | `number` | `30` | Rotten when task age > N days |

> **Ordering invariant**: `warning_days < stale_days < rotten_days`

### Complete Queue Example

```json
{
  "version": 2,
  "queue": {
    "file": ".ralph/queue.jsonc",
    "done_file": ".ralph/done.jsonc",
    "id_prefix": "RQ",
    "id_width": 4,
    "size_warning_threshold_kb": 500,
    "task_count_warning_threshold": 500,
    "max_dependency_depth": 10,
    "auto_archive_terminal_after_days": 7,
    "aging_thresholds": {
      "warning_days": 7,
      "stale_days": 14,
      "rotten_days": 30
    }
  }
}
```

---

## Plugin Configuration

The `plugins` section enables custom runners and processors.

> ⚠️ **Security Warning**: Plugins are NOT sandboxed. Only enable plugins from trusted sources.

### Structure

```json
{
  "plugins": {
    "plugins": {
      "<plugin-id>": {
        "enabled": true,
        "config": { /* opaque plugin config */ }
      }
    }
  }
}
```

### Per-Plugin Fields

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `enabled` | `boolean` | `false` | Enable/disable the plugin |
| `config` | `object` | `null` | Opaque plugin configuration |

### Plugin Directories

Plugins are discovered from:
- Project: `.ralph/plugins/<plugin_id>/plugin.json`
- Global: `~/.config/ralph/plugins/<plugin_id>/plugin.json`

Project-local plugin settings and project-scope plugin directories require repo trust (see [Repo execution trust](../configuration.md#repo-execution-trust)). In untrusted repos, Ralph ignores `.ralph/plugins/*` during runtime discovery.

### Example

```json
{
  "version": 2,
  "plugins": {
    "plugins": {
      "my.custom-runner": {
        "enabled": true,
        "runner": {
          "bin": "/usr/local/bin/custom-runner"
        },
        "config": {
          "api_key": "${CUSTOM_API_KEY}",
          "endpoint": "https://api.example.com"
        }
      }
    }
  }
}
```

### Plugin Commands

```bash
ralph plugin list                    # List discovered plugins
ralph plugin validate                # Validate plugin manifests
ralph plugin install <path>          # Install a plugin
ralph plugin uninstall <id>          # Uninstall a plugin
```

---

## Profile Configuration

Profiles enable quick switching between workflow presets. A profile is an `AgentConfig`-shaped patch applied over the base config.

### Defining Custom Profiles

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
    },
    "gemini-audit": {
      "runner": "gemini",
      "model": "gemini-3-pro-preview",
      "phases": 3
    }
  }
}
```

### Using Profiles

```bash
# Run with a profile
ralph run one --profile fast-local

# Scan with a profile
ralph scan --profile deep-review "security audit"

# Override settings while using a profile
ralph run one --profile fast-local --phases 2

# List available profiles
ralph config profiles list

# Show profile details
ralph config profiles show fast-local
```

### Profile Precedence

1. CLI flags (highest)
2. Task overrides (`task.agent.*`)
3. Selected profile
4. Base config (lowest)

---

## Environment Variables

Environment variables can be used within configuration files for dynamic values and secrets.

### Syntax

Use `${VAR_NAME}` or `$VAR_NAME` syntax in string values:

```json
{
  "agent": {
    "webhook": {
      "secret": "${WEBHOOK_SECRET}"
    }
  },
  "plugins": {
    "plugins": {
      "custom": {
        "config": {
          "api_key": "$API_KEY"
        }
      }
    }
  }
}
```

### Special Environment Variables

| Variable | Purpose |
|----------|---------|
| `XDG_CONFIG_HOME` | Override global config directory |

`RALPH_REPO_ROOT_OVERRIDE`, `RALPH_QUEUE_PATH_OVERRIDE`, and
`RALPH_DONE_PATH_OVERRIDE` are not supported.

### Best Practices

1. **Never commit secrets**: Use environment variables for API keys and tokens
2. **Use `.env` files**: Load env vars from `.env` before running Ralph
3. **Document required vars**: List required environment variables in your project README

---

## Validation

Ralph validates configuration on load and provides detailed error messages for invalid settings.

### Validation Rules

| Field | Validation |
|-------|------------|
| `version` | Must be `1` |
| `agent.phases` | Must be `1`, `2`, or `3` |
| `agent.iterations` | Must be `≥ 1` |
| `agent.session_timeout_hours` | Must be `≥ 1` |
| `parallel.workers` | Must be `≥ 2` (if set) |
| `parallel.max_push_attempts` | Must be `≥ 1` (if set) |
| `parallel.workspace_retention_hours` | Must be `≥ 1` (if set) |
| `queue.id_width` | Must be `≥ 1` (minimum 1) |
| `queue.*_threshold*` | Must be within documented ranges |
| Binary paths | Must be non-empty if specified |

### Validation Commands

```bash
# Show resolved configuration (validates on load)
ralph config show

# View configuration schema
ralph config schema
```

Note: Configuration validation happens implicitly when loading config. There is no separate `validate` subcommand.

### Common Validation Errors

```
Error: Unsupported config version: 1. Ralph requires version 2.
Solution: Set "version": 2 in your config file.

Error: Invalid agent.phases: 5. Supported values are 1, 2, or 3.
Solution: Change phases to 1, 2, or 3.

Error: Empty queue.id_prefix: prefix is required if specified.
Solution: Remove the field or set a non-empty prefix like "RQ".
```

---

## Complete Configuration Example

The full annotated example lives in
[Complete Configuration Example](configuration-example.md). Keep field behavior
and precedence in this guide; keep the long assembled sample in that child
reference.

## See Also

- [Main Configuration Documentation](../configuration.md)
- [CLI Reference](../cli.md)
- [Workflow Documentation](../workflow.md)
- [JSON Schema](../../schemas/config.schema.json)
