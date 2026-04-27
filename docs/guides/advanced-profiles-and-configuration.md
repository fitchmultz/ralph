# Advanced Profiles and Configuration
Status: Active
Owner: Maintainers
Source of truth: this document for advanced profile and configuration composition patterns
Parent: [Advanced Usage Guide](advanced.md)


Purpose: Deep-dive guidance for composing reusable profiles, layering configuration, applying per-task agent overrides, normalizing runner CLI behavior, and injecting instruction files.

---

## Table of Contents

1. [Custom Profiles](#custom-profiles)
2. [Advanced Configuration](#advanced-configuration)

---

## Custom Profiles

### Team Workflow Profiles

Define standardized profiles for team consistency:

```json
{
  "version": 2,
  "profiles": {
    "quick-fix": {
      "runner": "kimi",
      "model": "kimi-for-coding",
      "phases": 1
    },
    "standard-feature": {
      "runner": "claude",
      "model": "sonnet",
      "phases": 2
    },
    "critical-security": {
      "runner": "codex",
      "model": "gpt-5.3-codex",
      "phases": 3,
      "reasoning_effort": "high"
    },
    "code-review": {
      "runner": "claude",
      "model": "opus",
      "phases": 1,
      "instruction_files": ["~/.codex/REVIEW_GUIDELINES.md"]
    },
    "hotfix": {
      "runner": "kimi",
      "model": "kimi-for-coding",
      "phases": 1,
      "git_revert_mode": "enabled",
      "ci_gate": { "enabled": false }
    }
  }
}
```

### Environment-Specific Profiles

```json
{
  "profiles": {
    "ci-safe": {
      "runner": "kimi",
      "model": "kimi-for-coding",
      "phases": 1,
      "git_revert_mode": "enabled",
      "git_publish_mode": "off",
      "ci_gate": { "enabled": false }
    },
    "local-dev": {
      "runner": "claude",
      "model": "sonnet",
      "phases": 2,
      "git_revert_mode": "ask",
      "claude_permission_mode": "bypass_permissions"
    }
  }
}
```

### Profile Chaining Patterns

Use base profile + CLI overrides for flexibility:

```bash
# Start with deep-review profile, override for speed
ralph run one --profile deep-review --phases 2 --runner kimi

# Use CI-safe profile but enable auto-push for this run
ralph run loop --profile ci-safe --git-publish-mode commit_and_push
```

### Profile Inheritance Visualization

```
Base Config
    │
    ├── Profile: quick-fix
    │      ├── overrides: runner=kimi, phases=1
    │      └── inherits: model from base
    │
    ├── Profile: standard-feature
    │      ├── overrides: runner=claude, phases=2
    │      └── inherits: reasoning_effort from base
    │
    └── Profile: critical-security
           └── overrides: runner=codex, phases=3, effort=high
```

---

## Advanced Configuration

### JSONC (JSON with Comments)

Use comments in config for documentation:

```jsonc
{
  // Schema version - must be 2
  "version": 2,
  "agent": {
    /* Runner configuration.
       Built-in runner IDs: codex, opencode, gemini, claude, cursor, kimi, pi.
       Plugin runner IDs are also supported as non-empty strings. */
    "runner": "claude",
    "phases": 3, // 1 = single-pass, 2 = plan+implement, 3 = full workflow

    // CI gate settings
    "ci_gate": {
      "enabled": true,
      "argv": ["make", "ci"]
    },

    // Safety settings
    "git_revert_mode": "ask",
    "git_publish_mode": "commit_and_push"
  },
  "parallel": {
    // Workspace isolation
    "workspace_root": "/tmp/ralph-workspaces",
    "workers": 3
  }
}
```

### Layered Configuration Strategy

**Global config** (`~/.config/ralph/config.jsonc`):
```json
{
  "version": 2,
  "agent": {
    "runner": "claude",
    "model": "sonnet",
    "git_revert_mode": "ask"
  },
  "profiles": {
    "personal-default": {
      "runner": "kimi",
      "model": "kimi-for-coding"
    }
  }
}
```

**Project config** (`.ralph/config.jsonc`):
```json
{
  "version": 2,
  "agent": {
    "ci_gate": {
      "enabled": true,
      "argv": ["cargo", "test"]
    },
    "phases": 2
  },
  "profiles": {
    "team-standard": {
      "runner": "claude",
      "model": "sonnet",
      "phases": 2
    }
  }
}
```

**Resolution order:** CLI flags → Task overrides → Profile → Project config → Global config → Schema defaults

### Per-Task Agent Overrides

```json
{
  "id": "RQ-0001",
  "title": "Implement complex algorithm",
  "status": "todo",
  "agent": {
    "runner": "codex",
    "model": "gpt-5.3-codex",
    "model_effort": "high",
    "iterations": 2,
    "followup_reasoning_effort": "medium"
  }
}
```

### Runner CLI Normalization

Configure consistent behavior across runners:

```json
{
  "agent": {
    "runner_cli": {
      "defaults": {
        "output_format": "stream_json",
        "approval_mode": "auto_edits",
        "sandbox": "enabled",
        "unsupported_option_policy": "warn"
      },
      "runners": {
        "codex": {
          "sandbox": "disabled"
        },
        "claude": {
          "verbosity": "verbose",
          "approval_mode": "bypass_permissions"
        }
      }
    }
  }
}
```

### Instruction Files Injection

Inject custom instructions at the top of every prompt:

```json
{
  "agent": {
    "instruction_files": [
      "~/.codex/GLOBAL_GUIDELINES.md",
      "AGENTS.md",
      ".ralph/custom-instructions.md"
    ]
  }
}
```
