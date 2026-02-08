# Ralph Configuration Profiles

![Configuration Profiles](../assets/images/2026-02-07-11-32-24-profiles.png)

Purpose: Document Ralph's configuration profiles feature for quick workflow switching between different AI runner presets.

---

## Table of Contents

1. [Overview](#overview)
2. [Built-in Profiles](#built-in-profiles)
3. [Custom Profiles](#custom-profiles)
4. [Profile Precedence](#profile-precedence)
5. [Using Profiles](#using-profiles)
6. [Profile Inheritance](#profile-inheritance)
7. [Inspecting Profiles](#inspecting-profiles)
8. [Practical Examples](#practical-examples)

---

## Overview

Configuration profiles enable **quick switching between different workflow presets** without manually editing config files or passing many CLI flags for each invocation.

### Why Use Profiles?

| Use Case | Benefit |
|----------|---------|
| Different task complexity levels | Switch between quick fixes and deep analysis |
| Runner/model preferences | Use fast models for simple tasks, powerful models for complex ones |
| Workflow experimentation | Easily test different phase configurations |
| Team standardization | Share consistent presets across team members |
| CI/CD integration | Use deterministic profiles in automation |

### What is a Profile?

A profile is an **AgentConfig-shaped patch** that is applied over the base `agent` configuration when selected via `--profile <NAME>`. Profiles can specify:

- `runner`: The AI runner to use (codex, claude, kimi, etc.)
- `model`: The specific model ID
- `phases`: Number of execution phases (1, 2, or 3)
- `reasoning_effort`: Reasoning depth for Codex (low, medium, high, xhigh)
- Any other `AgentConfig` field

---

## Built-in Profiles

Ralph includes two **always-available** built-in profiles:

### `quick` Profile

Optimized for **fast, single-pass execution**:

| Setting | Value | Description |
|---------|-------|-------------|
| `runner` | `kimi` | Kimi CLI runner |
| `model` | `kimi-for-coding` | Optimized coding model |
| `phases` | `1` | Single-pass execution |

**Best for:**
- Quick bug fixes
- Typo corrections
- Small refactors
- Simple documentation updates
- Urgent patches

```bash
# Use the quick profile
ralph run one --profile quick
ralph scan --profile quick "quick fixes"
```

### `thorough` Profile

Optimized for **deep, multi-phase execution** with powerful models:

| Setting | Value | Description |
|---------|-------|-------------|
| `runner` | `claude` | Claude CLI runner |
| `model` | `opus` | Most capable Claude model |
| `phases` | `3` | Full 3-phase workflow |

**Best for:**
- Complex feature implementations
- Architecture changes
- Security audits
- Critical code reviews
- Design refactoring

```bash
# Use the thorough profile
ralph run one --profile thorough
ralph scan --profile thorough "security audit"
```

### Built-in Profile Comparison

```
┌─────────────────────────────────────────────────────────────────┐
│                    Built-in Profiles                             │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  quick (fast)                     thorough (deep)               │
│  ─────────────────                ─────────────────             │
│  runner: kimi                     runner: claude                │
│  model: kimi-for-coding           model: opus                   │
│  phases: 1 (single-pass)          phases: 3 (plan/impl/review)  │
│                                                                 │
│  Use for:                         Use for:                      │
│  • Quick fixes                    • Complex features            │
│  • Small refactors                • Security audits             │
│  • Typo corrections               • Architecture changes        │
│  • Urgent patches                 • Critical reviews            │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

---

## Custom Profiles

Define your own profiles in `.ralph/config.json` under the `profiles` key.

### Profile Structure

```json
{
  "version": 1,
  "profiles": {
    "<profile-name>": {
      "runner": "<runner-name>",
      "model": "<model-id>",
      "phases": <1|2|3>,
      "reasoning_effort": "<low|medium|high|xhigh>",
      "...": "..."
    }
  }
}
```

### Custom Profile Examples

#### Example 1: Codex Review Profile

```json
{
  "version": 1,
  "profiles": {
    "codex-review": {
      "runner": "codex",
      "model": "gpt-5.3-codex",
      "phases": 2,
      "reasoning_effort": "high"
    }
  }
}
```

Usage:
```bash
ralph run one --profile codex-review
```

#### Example 2: Gemini Audit Profile

```json
{
  "version": 1,
  "profiles": {
    "gemini-audit": {
      "runner": "gemini",
      "model": "gemini-3-pro-preview",
      "phases": 3,
      "reasoning_effort": "high"
    }
  }
}
```

Usage:
```bash
ralph scan --profile gemini-audit "production security audit"
```

#### Example 3: Multiple Custom Profiles

```json
{
  "version": 1,
  "profiles": {
    "fast-fix": {
      "runner": "kimi",
      "model": "kimi-for-coding",
      "phases": 1
    },
    "standard": {
      "runner": "claude",
      "model": "sonnet",
      "phases": 2
    },
    "deep-analysis": {
      "runner": "claude",
      "model": "opus",
      "phases": 3,
      "reasoning_effort": "high"
    },
    "codex-iterate": {
      "runner": "codex",
      "model": "gpt-5.3-codex",
      "phases": 2,
      "reasoning_effort": "high",
      "iterations": 3
    }
  }
}
```

Usage:
```bash
ralph run one --profile fast-fix      # Quick fix
ralph run one --profile standard      # Normal workflow
ralph run one --profile deep-analysis # Thorough review
ralph run one --profile codex-iterate # Multi-iteration
```

### Overriding Built-in Profiles

User-defined profiles with the **same name as built-ins** override the built-in:

```json
{
  "version": 1,
  "profiles": {
    "quick": {
      "runner": "codex",
      "model": "gpt-5.2-codex",
      "phases": 1
    }
  }
}
```

Now `ralph run one --profile quick` uses Codex instead of Kimi.

---

## Profile Precedence

When a profile is selected, the final configuration is computed in this order (highest to lowest):

```
1. CLI flags (e.g., --runner, --model, --phases, --effort)
2. Task overrides (task.agent.* in the queue)
3. Selected profile (config-defined or built-in)
4. Base config (merged global + project config)
5. Schema defaults
```

### Precedence Rules

| Rule | Explanation |
|------|-------------|
| CLI flags always win | Override everything else |
| Task overrides beat profiles | Per-task agent settings take precedence |
| Profiles override base config | Applied on top of merged global/project config |
| User profiles beat built-ins | Same name = user profile wins |
| Base config fills gaps | Unspecified profile fields inherit from config |

### Visual Precedence Flow

```
┌─────────────────────────────────────────────────────────────────┐
│              Configuration Resolution Flow                       │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  ┌──────────────┐                                               │
│  │  CLI Flags   │ ◄──── Highest precedence                     │
│  │  --phases 2  │                                               │
│  └──────┬───────┘                                               │
│         │                                                        │
│  ┌──────▼───────────┐                                           │
│  │  Task Overrides  │                                           │
│  │  task.agent.*    │                                           │
│  └──────┬───────────┘                                           │
│         │                                                        │
│  ┌──────▼───────────┐                                           │
│  │ Selected Profile │ ◄──── --profile <name>                    │
│  │ quick/thorough   │                                           │
│  └──────┬───────────┘                                           │
│         │                                                        │
│  ┌──────▼───────────┐                                           │
│  │   Base Config    │ ◄──── .ralph/config.json                  │
│  │  (merged global) │         + ~/.config/ralph/config.json      │
│  └──────┬───────────┘                                           │
│         │                                                        │
│  ┌──────▼───────────┐                                           │
│  │ Schema Defaults  │ ◄──── Lowest precedence                    │
│  └──────────────────┘                                           │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

---

## Using Profiles

### CLI Flag

Select a profile using the `--profile` flag:

```bash
# Run with the quick profile
ralph run one --profile quick

# Run with the thorough profile
ralph run one --profile thorough

# Scan with a custom profile
ralph scan --profile gemini-audit "security audit"

# Run multiple tasks with a profile
ralph run loop --profile quick --max-tasks 5
```

### Combining with CLI Overrides

Override specific settings while using a profile:

```bash
# Use quick profile but change phases and runner
ralph run one --profile quick --phases 2 --runner claude

# Use thorough profile but change model
ralph run one --profile thorough --model sonnet

# Use custom profile with phase override
ralph run one --profile codex-review --phases 3
```

### Commands Supporting `--profile`

| Command | Profile Support | Notes |
|---------|-----------------|-------|
| `ralph run one` | ✅ Yes | Full support |
| `ralph run loop` | ✅ Yes | Full support |
| `ralph scan` | ✅ Yes | Full support |
| `ralph task` | ✅ Yes | Full support |

---

## Profile Inheritance

Profiles are merged into the base config using **leaf-wise merge semantics**:

- `Some(value)` in the profile **overrides** the base config
- `None` or **omitted** fields **inherit** from the base config

### Partial Profile Example

A profile only needs to specify the fields it wants to change:

```json
{
  "profiles": {
    "fast-fix": {
      "phases": 1
    }
  }
}
```

This profile:
- Changes `phases` to `1`
- Keeps `runner` from base config
- Keeps `model` from base config
- Keeps all other `agent` settings

### Inheritance Chain

```
Base Config (agent.*)
    │
    ▼
┌──────────────────────────────────────┐
│ Profile Applied (leaf-wise merge)    │
│ - Some values override               │
│ - None/omitted values inherit        │
└──────────────────────────────────────┘
    │
    ▼
Task Overrides (task.agent.*)
    │
    ▼
CLI Flags (--runner, --model, etc.)
    │
    ▼
Final Resolved Config
```

### Example Inheritance Walkthrough

**Base config** (`.ralph/config.json`):
```json
{
  "version": 1,
  "agent": {
    "runner": "claude",
    "model": "sonnet",
    "phases": 3,
    "reasoning_effort": "medium"
  },
  "profiles": {
    "codex-high": {
      "runner": "codex",
      "model": "gpt-5.3-codex",
      "reasoning_effort": "high"
    }
  }
}
```

**Using the profile**:
```bash
ralph run one --profile codex-high
```

**Final resolved config**:
```json
{
  "runner": "codex",           // From profile
  "model": "gpt-5.3-codex",    // From profile
  "phases": 3,                 // Inherited from base
  "reasoning_effort": "high"   // From profile
}
```

---

## Inspecting Profiles

### List Available Profiles

```bash
ralph config profiles list
```

Output example:
```
Available profiles:
  quick (built-in) - runner=kimi, model=kimi-for-coding, phases=1
  thorough (built-in) - runner=claude, model=opus, phases=3
  codex-high - runner=codex, model=gpt-5.3-codex, effort=high
  fast-fix - phases=1
```

### Show Specific Profile

```bash
# Show built-in profile
ralph config profiles show quick

# Show custom profile
ralph config profiles show codex-high
```

Output example:
```yaml
Profile: quick
Source: built-in

runner: kimi
model: kimi-for-coding
phases: 1
```

### Show Resolved Configuration

View the fully resolved config (with profile applied):

```bash
# Human-readable YAML
ralph config show

# Machine-readable JSON
ralph config show --format json
```

---

## Practical Examples

### Example 1: Daily Workflow Profiles

**Config** (`.ralph/config.json`):
```json
{
  "version": 1,
  "agent": {
    "runner": "claude",
    "model": "sonnet",
    "phases": 2
  },
  "profiles": {
    "fix": {
      "runner": "kimi",
      "model": "kimi-for-coding",
      "phases": 1
    },
    "feature": {
      "phases": 3,
      "model": "opus"
    },
    "audit": {
      "runner": "codex",
      "model": "gpt-5.3-codex",
      "phases": 2,
      "reasoning_effort": "high"
    }
  }
}
```

**Usage**:
```bash
# Quick bug fix
ralph run one --profile fix

# New feature (uses Claude/opus with 3 phases)
ralph run one --profile feature

# Security audit
ralph scan --profile audit "security review"
```

### Example 2: Team Standardization

**Global config** (`~/.config/ralph/config.json`):
```json
{
  "version": 1,
  "profiles": {
    "team-standard": {
      "runner": "claude",
      "model": "sonnet",
      "phases": 2
    },
    "team-deep": {
      "runner": "claude",
      "model": "opus",
      "phases": 3
    }
  }
}
```

All team members can use consistent profiles:
```bash
ralph run one --profile team-standard
```

### Example 3: CI/CD Integration

**Profile for automated checks**:
```json
{
  "version": 1,
  "profiles": {
    "ci-check": {
      "runner": "kimi",
      "model": "kimi-for-coding",
      "phases": 1,
      "git_revert_mode": "enabled",
      "git_commit_push_enabled": false
    }
  }
}
```

**CI workflow**:
```yaml
# .github/workflows/ralph-check.yml
jobs:
  ralph-check:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - name: Run Ralph CI Check
        run: ralph run one --profile ci-check --non-interactive
```

### Example 4: Mixed Runner Workflow with Profiles

**Config**:
```json
{
  "version": 1,
  "profiles": {
    "plan-with-codex": {
      "runner": "codex",
      "model": "gpt-5.3-codex",
      "phases": 1
    },
    "implement-with-kimi": {
      "runner": "kimi",
      "model": "kimi-for-coding",
      "phases": 2
    }
  }
}
```

**Workflow**:
```bash
# Phase 1: Generate plan with Codex
ralph run one --profile plan-with-codex

# Phase 2: Implement with Kimi (using cached plan)
ralph run one --profile implement-with-kimi
```

### Example 5: Debugging Profile Issues

```bash
# 1. Check available profiles
ralph config profiles list

# 2. Inspect specific profile
ralph config profiles show quick

# 3. View resolved config (with profile)
ralph config show

# 4. Run with verbose output to see profile selection
ralph --verbose run one --profile quick
```

---

## See Also

- [Configuration](../configuration.md) - Full configuration reference
- [CLI Reference](../cli.md) - Complete CLI documentation
- [Phases](phases.md) - Phase system documentation
- [Runners](runners.md) - Runner selection and configuration
