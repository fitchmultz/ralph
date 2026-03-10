# Ralph Configuration Profiles

![Custom Profiles](../assets/images/2026-03-10-12-11-39-profiles.png)

Purpose: Document Ralph's custom configuration profiles feature for quick workflow switching between user-defined AI runner presets.

---

## Overview

Configuration profiles let you name a reusable `agent` patch and apply it with `--profile <NAME>`.

Profiles are:
- Defined by you in `.ralph/config.jsonc` or `~/.config/ralph/config.jsonc`
- Applied before CLI overrides are resolved
- Useful for standardizing repeatable workflows without typing many flags each time

Profiles are not built in. If you want names like `quick` or `thorough`, define them explicitly in your config.

## Example Profiles

```jsonc
{
  "version": 1,
  "agent": {
    "runner": "codex",
    "model": "gpt-5.4",
    "phases": 2
  },
  "profiles": {
    "fast-local": {
      "phases": 1,
      "reasoning_effort": "low"
    },
    "deep-review": {
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

Common patterns:
- `fast-local`: quick single-pass runs
- `deep-review`: full plan/implement/review flow
- `gemini-audit`: alternate runner for a specific class of scans

## Using Profiles

```bash
# Run one task with a configured profile
ralph run one --profile fast-local

# Scan with a deeper profile
ralph scan --profile deep-review "security audit"

# Override profile settings for one invocation
ralph run one --profile fast-local --phases 2 --runner claude

# Inspect configured profiles
ralph config profiles list
ralph config profiles show fast-local
```

## Precedence and Inheritance

When a profile is selected, Ralph resolves settings in this order:

1. CLI flags
2. Task overrides (`task.agent.*`)
3. Selected profile
4. Base config

Profile patches use the same leaf-wise merge rules as the rest of config:
- fields you set in the profile override base config
- fields you omit inherit from base config

That means a profile can stay small:

```jsonc
{
  "profiles": {
    "fast-local": {
      "phases": 1
    }
  }
}
```

## Recreating Old Names

If your team still wants `quick` or `thorough`, define them directly:

```jsonc
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

This is now just a normal custom-profile pattern, not special built-in behavior.

## Troubleshooting

- `Unknown profile`: the selected name is not defined in your config. Run `ralph config profiles list` to confirm what exists.
- `No profiles configured`: add a `profiles` object to `.ralph/config.jsonc` or `~/.config/ralph/config.jsonc`.
- Need one-off changes: keep the profile small and override the rest with CLI flags.
