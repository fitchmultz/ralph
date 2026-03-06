# Ralph Documentation

Ralph is a Rust CLI for running AI agent loops against a structured JSON task queue.

## Start Here

- [README](../README.md): product pitch, why it matters, and first end-to-end workflow
- [Portfolio Guide](../PORTFOLIO.md): fast reviewer path
- [Architecture Overview](architecture.md): components, data/control flow, trust boundaries
- [Quick Start](quick-start.md): install, initialize, create first task, run it
- [CLI Reference](cli.md): command map + high-value workflows
- [Configuration](configuration.md): config schema, precedence, and defaults
- [Queue and Tasks](queue-and-tasks.md): task model and queue semantics
- [Reviewer Smoke Test](guides/reviewer-smoke-test.md): deterministic evaluation path

## Core Command Areas

- `ralph run`: supervised execution (`one`, `loop`, `resume`, `parallel`)
- `ralph task`: task creation, lifecycle, relations, templates, batch ops
- `ralph queue`: queue inspection, validation, analytics, import/export
- `ralph scan`: repository scanning and task discovery
- `ralph prompt`: prompt rendering/export/sync/diff
- `ralph doctor`: readiness diagnostics
- `ralph plugin`: plugin lifecycle
- `ralph daemon` + `ralph watch`: background automation
- `ralph webhook`: test/status/replay for event delivery

## Reviewer Path

Use these if you are evaluating whether Ralph feels public-ready:

- [README](../README.md)
- [Portfolio Guide](../PORTFOLIO.md)
- [Reviewer Smoke Test](guides/reviewer-smoke-test.md)
- [Architecture Overview](architecture.md)
- [Public Readiness Checklist](guides/public-readiness.md)
- [Security Model](security-model.md)

## Reference Docs

- [CLI Reference](cli.md)
- [Configuration](configuration.md)
- [CI and Test Strategy](guides/ci-strategy.md)
- [Troubleshooting](troubleshooting.md)
- [Support Policy](support-policy.md)
- [Versioning Policy](versioning-policy.md)

## Maintainer Runbooks

- [Release Runbook](guides/release-runbook.md)
- [Current State Baseline](guides/current-state-baseline.md)
- [Public Release Hardening Investigation](guides/public-release-hardening-investigation.md)
- [History Cleanup Execution Plan](guides/history-cleanup-execution-plan.md)

## Public Evidence Pack

- [Release Readiness Report](guides/release-readiness-report.md)
- [Reviewer Smoke Test](guides/reviewer-smoke-test.md)
- [Role Evidence Index](role-evidence/index.md)
- [Portfolio Guide](../PORTFOLIO.md)

## Runtime Paths (Defaults)

- Queue: `.ralph/queue.jsonc` (`.json` fallback)
- Done archive: `.ralph/done.jsonc` (`.json` fallback)
- Project config: `.ralph/config.jsonc` (`.json` fallback)
- Prompt overrides: `.ralph/prompts/`

## Validation and CI

> GNU Make >= 4 is required for project targets. On macOS, install with `brew install make` and use `gmake` unless your PATH already exposes GNU Make as `make`.

Before merging documentation or code changes:

```bash
make ci-fast
make agent-ci
# Optional shared-workstation caps: RALPH_CI_JOBS=4 make agent-ci
```

Full Rust release gate:

```bash
make ci
```

Ship gate (when macOS app changes are in scope):

```bash
make macos-ci
# Optional caps: RALPH_CI_JOBS=4 RALPH_XCODE_JOBS=4 make macos-ci
```
