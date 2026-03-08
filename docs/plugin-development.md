# Plugin Development Guide

![Plugin Architecture](assets/images/2026-02-07-plugin-architecture.png)

Purpose: Guide for developing custom Ralph plugins (runners and task processors).

## Quick Start

The fastest way to start building a plugin is with the scaffold command:

```bash
# Scaffold a new plugin in the project scope (default)
ralph plugin init my.plugin

# Scaffold with only runner support
ralph plugin init my.plugin --with-runner

# Scaffold with only processor support
ralph plugin init my.plugin --with-processor

# Scaffold in the global scope
ralph plugin init my.plugin --scope global

# Preview what would be created without writing files
ralph plugin init my.plugin --dry-run
```

This creates:
- `plugin.json` - A valid manifest that passes validation
- `runner.sh` - A runner stub script (if runner support requested)
- `processor.sh` - A processor stub script (if processor support requested)

By default, both runner and processor scripts are created. Plugins are created in:
- **Project scope**: `.ralph/plugins/<plugin_id>/`
- **Global scope**: `~/.config/ralph/plugins/<plugin_id>/`

**Important**: The plugin is NOT automatically enabled. To enable it, add to your config:

```json
{
  "plugins": {
    "plugins": {
      "my.plugin": {
        "enabled": true
      }
    }
  }
}
```

Then validate your plugin:

```bash
ralph plugin validate --id my.plugin
```

## Overview

Ralph's plugin system allows extending the tool with custom runners and task processors without modifying the core codebase. Plugins are discovered from:

- **Global**: `~/.config/ralph/plugins/<plugin_id>/`
- **Project**: `.ralph/plugins/<plugin_id>/`

Project plugins override global plugins with the same ID.

## Plugin Structure

A plugin is a directory containing at least a `plugin.json` manifest file:

```
my-plugin/
├── plugin.json       # Required: Plugin manifest
├── runner.sh         # Optional: Runner executable
└── processor.sh      # Optional: Processor executable
```

When you run `ralph plugin init`, the scaffold generates this layout with:
- A valid `plugin.json` with all required fields
- Executable shell scripts with `--help` output and protocol documentation
- Proper file permissions (executable bit set on Unix systems)

## Plugin Manifest (`plugin.json`)

```json
{
  "api_version": 1,
  "id": "my.plugin",
  "version": "1.0.0",
  "name": "My Plugin",
  "description": "A custom runner plugin",
  "runner": {
    "bin": "runner.sh",
    "supports_resume": true,
    "default_model": "gpt-5.2"
  },
  "processors": {
    "bin": "processor.sh",
    "hooks": ["validate_task", "pre_prompt", "post_run"]
  }
}
```

### Manifest Fields

| Field | Required | Description |
|-------|----------|-------------|
| `api_version` | Yes | Must be `1` (current API version) |
| `id` | Yes | Unique plugin identifier (no spaces, no path separators) |
| `version` | Yes | SemVer version string |
| `name` | Yes | Human-readable name |
| `description` | No | Brief description |
| `runner` | No | Runner configuration object |
| `runner.bin` | Yes* | Path to runner executable (relative to plugin dir) |
| `runner.supports_resume` | No | Whether the runner supports session resumption |
| `runner.default_model` | No | Default model when none specified |
| `processors` | No | Processor configuration object |
| `processors.bin` | Yes* | Path to processor executable |
| `processors.hooks` | Yes* | List of supported hooks |

*Required if the respective section is present.

### Supported Hooks (for processors)

- `validate_task`: Called to validate task structure
- `pre_prompt`: Called before prompt is sent to runner
- `post_run`: Called after runner execution completes

### Processor Hook Runtime Semantics

The following semantics define how processor hooks are invoked during `ralph run`:

**Deterministic Chaining Order**
- Enabled processor plugins are executed in ascending lexicographic order by `plugin_id`
- Rationale: Both discovery and config use `BTreeMap` keyed by id; iteration order is stable

**Hook Invocation Points**

| Hook | When Invoked | Failure Behavior |
|------|--------------|------------------|
| `validate_task` | After task selection but before marking `doing` | Fatal - aborts run before any work begins |
| `pre_prompt` | After prompt compilation but before runner invocation | Fatal - aborts before spawning runner |
| `post_run` | After each successful runner execution (including CI continues) | Fatal - aborts workflow at point of failure |

**Important Notes on `post_run`**:
- Invoked after every successful runner `run` (normal execution)
- Invoked after each successful `resume` / Continue (CI gate enforcement)
- May run multiple times in a single overall task execution

**Environment Variables**

All processor hooks receive:
- `RALPH_PLUGIN_ID`: The plugin ID (e.g., `my.plugin`)
- `RALPH_PLUGIN_CONFIG_JSON`: Plugin config blob as JSON string (use `{}` when missing)

**Hook Protocol**

For each invoked plugin/hook:
- Binary: Resolved from manifest `processors.bin`
- Working directory: Repository root
- Arguments: `<HOOK> <TASK_ID> <FILEPATH>`
- File contents:
  - `validate_task`: Full task JSON
  - `pre_prompt`: Prompt text (plugin may modify in place)
  - `post_run`: Runner stdout (NDJSON text)

**Runtime Limits**
- Processor hooks run through Ralph's managed subprocess service, not raw shell execution
- Hook stdout/stderr capture is bounded in memory; large output tails may be truncated in diagnostics
- Hooks have a bounded execution window; a hung hook is interrupted with `SIGINT` first and escalated to hard kill if it ignores shutdown
- Hook authors should handle `SIGINT` cleanly and exit promptly during cleanup
- Non-zero exit remains a fatal hook failure

**Exit Code Contract**
- `0`: Success
- Non-zero: Failure (abort with actionable error including plugin_id, hook name, and exit code)

## Enabling Plugins

Plugins are **disabled by default** for security. Enable via config:

```json
{
  "version": 1,
  "plugins": {
    "plugins": {
      "my.plugin": {
        "enabled": true,
        "runner": {
          "bin": "custom-runner"
        },
        "config": {
          "my_setting": "value"
        }
      }
    }
  }
}
```

Per-plugin configuration is passed through to the plugin via environment variables.

## Runner Protocol

Runner plugins receive commands via arguments and prompts via stdin.

### Environment Variables

When your runner is invoked, these environment variables are set:

| Variable | Description |
|----------|-------------|
| `RALPH_PLUGIN_ID` | The plugin ID |
| `RALPH_PLUGIN_CONFIG_JSON` | Opaque plugin config blob (JSON string) |
| `RALPH_RUNNER_CLI_JSON` | Resolved runner CLI options (JSON) |

### Run Command

```bash
your-runner run --model <model> --output-format stream-json [--session <id>]
```

The prompt is passed via stdin. The runner MUST output newline-delimited JSON:

```json
{"type": "text", "content": "Hello"}
{"type": "tool_call", "name": "write", "arguments": {"path": "file.txt", "content": "data"}}
{"type": "finish", "session_id": "RQ-0001-p2-1704153600"}
```

### Resume Command

```bash
your-runner resume --session <id> --model <model> --output-format stream-json <message>
```

The message is passed as the final argument. Output format is the same as `run`.

## Example: Custom Logger Plugin

Here's a minimal example of a task completion logger plugin:

**Directory structure:**
```
logger-plugin/
├── plugin.json
└── logger.sh
```

**plugin.json:**
```json
{
  "api_version": 1,
  "id": "example.logger",
  "version": "1.0.0",
  "name": "Task Logger",
  "description": "Logs task completions to a file",
  "processors": {
    "bin": "logger.sh",
    "hooks": ["post_run"]
  }
}
```

**logger.sh:**
```bash
#!/bin/bash
# logger.sh - Example processor plugin

HOOK=$1
TASK_ID=$2
shift 2

case "$HOOK" in
  post_run)
    echo "$(date -Iseconds) Task $TASK_ID completed" >> "$RALPH_LOG_PATH"
    ;;
esac
```

**Enable and configure:**
```json
{
  "plugins": {
    "plugins": {
      "example.logger": {
        "enabled": true,
        "config": {
          "log_path": "/path/to/task.log"
        }
      }
    }
  }
}
```

## Installing Plugins

Install from a local directory:

```bash
# Install to project scope
ralph plugin install ./my-plugin --scope project

# Install to global scope
ralph plugin install ./my-plugin --scope global
```

Install does NOT auto-enable the plugin for security. Enable manually in config.

To create a new plugin from scratch, use `ralph plugin init` (see Quick Start section).

## Managing Plugins

```bash
# List discovered plugins
ralph plugin list

# List as JSON
ralph plugin list --json

# Validate plugin manifests
ralph plugin validate

# Validate specific plugin
ralph plugin validate --id my.plugin

# Uninstall plugin
ralph plugin uninstall my.plugin --scope project
```

## Security Considerations

1. **Plugins are NOT sandboxed**: Enabling a plugin is equivalent to trusting it with full system access.

2. **Enable explicitly**: Plugins must be explicitly enabled in config. Discovery alone does not activate plugins.

3. **Validate before installing**: Review plugin code before installation:
   ```bash
   ralph plugin validate --id my.plugin
   ```

4. **Project vs Global**: Project plugins override global plugins, but only in trusted repos. Untrusted repos ignore `.ralph/plugins/*` at runtime.

## Debugging

Enable verbose logging to see plugin-related activity:

```bash
RUST_LOG=debug ralph plugin list
```

Check plugin discovery:

```bash
# Shows which directories are checked
ralph plugin list
# If no plugins found, it prints the checked directories
```

Verify environment variables in your plugin:

```bash
#!/bin/bash
# Debug script to see what Ralph passes
env | grep RALPH_ > /tmp/ralph_plugin_env.txt
echo "Environment written to /tmp/ralph_plugin_env.txt"
```

## Best Practices

1. **Use semantic versioning** for your plugin versions
2. **Handle missing config gracefully** - the `RALPH_PLUGIN_CONFIG_JSON` may be `{}`
3. **Exit codes matter** - non-zero exit codes are treated as failures
4. **Idempotent operations** - runner resume should be idempotent
5. **Document your config** - include expected config fields in your plugin README

## API Version Compatibility

The current plugin API version is `1`. Ralph will reject plugins with incompatible API versions. When Ralph updates to API version 2, plugins will need to update their manifests.

## Troubleshooting

**Plugin not discovered:**
- Verify the directory structure: `<plugin_root>/<plugin_id>/plugin.json`
- Check file permissions
- Run `ralph plugin list` to see searched directories

**Plugin validation fails:**
- Check `api_version` is `1`
- Ensure plugin ID has no spaces or path separators
- Verify required fields are present

**Plugin not executing:**
- Verify plugin is enabled in config: `plugins.plugins.<id>.enabled = true`
- Check the executable exists and is executable
- Look at stderr output for error messages

**Runner not found:**
- Verify `runner.bin` path in manifest
- Path must stay relative to the plugin directory
- Config-level runner/processor binary overrides are not supported
