# Advanced Usage Guide
Status: Active
Owner: Maintainers
Source of truth: this document for advanced usage navigation; linked child guides are source of truth for their stated domains
Parent: [Ralph Documentation](../index.md)


Purpose: Navigation hub for advanced Ralph workflows, configuration patterns, integrations, automation, optimization, and troubleshooting.

---

## Use This Guide

Start here when you need power-user workflows that go beyond the quick start and CLI reference. Each child guide owns a focused advanced domain so changes stay localized and file-size warnings stay quiet.

## Advanced Domains

| Domain | Guide | Use when you need... |
|--------|-------|----------------------|
| Workflow execution | [Advanced Workflows](advanced-workflows.md) | multi-phase strategy, parallel execution, workflow optimization, runner retry/session tuning |
| Profiles and configuration | [Advanced Profiles and Configuration](advanced-profiles-and-configuration.md) | team profiles, profile chaining, JSONC examples, layered config, per-task overrides, runner CLI normalization, instruction files |
| Plugins and automation | [Advanced Plugins and Automation](advanced-plugins-and-automation.md) | custom runner/processor plugins, plugin debugging, daemon/watch setup, service managers, CI/CD examples, webhooks |
| Troubleshooting and reference | [Advanced Troubleshooting and Reference](advanced-troubleshooting.md) | session recovery, lock issues, parallel recovery, CI failures, quick command/config/file-location reference |

## Related Canonical References

- [Configuration](../configuration.md): complete configuration schema, precedence, and defaults.
- [CLI Reference](../cli.md): command map and everyday command examples.
- [CI and Test Strategy](ci-strategy.md): canonical validation tiers, including `make check-file-size-limits`.
- [Project Operating Constitution](project-operating-constitution.md): source-of-truth, cutover, documentation, UX, and validation rules.
