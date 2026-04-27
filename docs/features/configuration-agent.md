# Agent and Runner Configuration
Status: Active
Owner: Maintainers
Source of truth: this document for feature-level agent/runner configuration guidance
Parent: [Configuration Feature Guide](configuration.md)

Use this guide when choosing how Ralph invokes AI runners. For exact field names, defaults, and validation rules, use [Agent Configuration](../configuration.md#agent-configuration).

---

## Common Decisions

Most teams tune these first:

- Runner and model (`agent.runner`, `agent.model`)
- Workflow depth (`agent.phases`)
- Iteration count (`agent.iterations`)
- Reasoning depth (`agent.reasoning_effort`, `agent.followup_reasoning_effort`)
- Additional injected guidance (`agent.instruction_files`)

Related feature docs:

- [Runners](./runners.md)
- [Phases](./phases.md)
- [Supervision](./supervision.md)

---

## Minimal Safe Baseline

```jsonc
{
  "version": 2,
  "agent": {
    "runner": "codex",
    "model": "gpt-5.4",
    "phases": 3,
    "reasoning_effort": "medium",
    "ci_gate": {
      "enabled": true,
      "argv": ["make", "ci"]
    }
  }
}
```

---

## Runner Binary Overrides

You can override CLI executable names/paths for runners (`*_bin` fields under `agent`).

Operational rule: project-level binary overrides are execution-sensitive settings and require repository trust before project values are honored.

Canonical details:

- [Repo execution trust](../configuration.md#repo-execution-trust)
- [Agent Configuration](../configuration.md#agent-configuration)

---

## Permission and Publication Safety

Operator-sensitive controls include:

- `agent.claude_permission_mode`
- `agent.runner_cli.*.approval_mode`
- `agent.git_publish_mode`
- `agent.git_revert_mode`

Treat permissive modes (`yolo`, bypass-style approvals, publish-on-run) as high-trust settings and scope them intentionally.

> Note: Codex approval behavior is managed through Codex-native config. Ralph does not force Codex approval flags.

---

## CI Gate

`agent.ci_gate` is the execution validation gate.

- Keep `enabled: true` unless you have a documented temporary exception.
- `ci_gate.argv` is argv-only (no shell-string launchers like `sh -c`).

Canonical details: [Agent Configuration](../configuration.md#agent-configuration).

---

## Runner CLI Normalization

`agent.runner_cli` supports:

- Global defaults (`agent.runner_cli.defaults`)
- Per-runner overrides (`agent.runner_cli.runners.<runner>.*`)

Typical controls:

- `output_format`
- `verbosity`
- `approval_mode`
- `sandbox`
- `plan_mode`
- `unsupported_option_policy`

Canonical reference: [agent.runner_cli](../configuration.md#agentrunner_cli).

---

## Phase Overrides and Retry

Use `agent.phase_overrides` for phase-specific runner/model/reasoning changes:

```jsonc
{
  "version": 2,
  "agent": {
    "runner": "codex",
    "model": "gpt-5.4",
    "phase_overrides": {
      "phase1": { "reasoning_effort": "high" },
      "phase2": { "model": "gpt-5.4" },
      "phase3": { "reasoning_effort": "high" }
    }
  }
}
```

Use `agent.runner_retry` for bounded transient-failure retries (attempt/backoff/jitter controls).

Canonical references:

- [agent.phase_overrides](../configuration.md#agentphase_overrides)
- [agent.runner_retry](../configuration.md#agentrunner_retry)

---

## See Also

- [Configuration Feature Guide](configuration.md)
- [Main Configuration Reference](../configuration.md)
- [Runners](./runners.md)
- [Phases](./phases.md)
- [Supervision](./supervision.md)
