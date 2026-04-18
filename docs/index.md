# Ralph Documentation

Ralph is a Rust CLI for running AI agent loops against a structured JSON task queue.

## Start Here

- [README](../README.md): product overview, first end-to-end workflow, and verification path
- [Evaluator Path](guides/evaluator-path.md): fastest reviewer-friendly path through install, queue inspection, and local verification
- [Architecture Overview](architecture.md): components, data/control flow, trust boundaries
- [Quick Start](quick-start.md): install, initialize, create first task, run it
- [CLI Reference](cli.md): command map + high-value workflows
- [Machine Contract](machine-contract.md): versioned app/automation JSON API
- [Roadmap](roadmap.md): canonical near-term execution order for active follow-up work
- [Configuration](configuration.md): config schema, precedence, and defaults
- [PRD Specs](prd/ralph-task-decompose.md): feature-level product requirements
- [Queue and Tasks](queue-and-tasks.md): task model and queue semantics
- [Local Smoke Test](guides/local-smoke-test.md): deterministic install and verification path
- [Stack Audit (2026-03)](guides/stack-audit-2026-03.md): current toolchain/dependency inventory and best-practice review

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

## Verification and Operations

Use these when you want to validate a clone, understand the operational model, or prepare for a public release:

- [README](../README.md)
- [Evaluator Path](guides/evaluator-path.md)
- [Local Smoke Test](guides/local-smoke-test.md)
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
- [Full Release Guide](releasing.md)

## Runtime Paths (Defaults)

- Queue: `.ralph/queue.jsonc`
- Done archive: `.ralph/done.jsonc`
- Project config: `.ralph/config.jsonc`
- Prompt overrides: `.ralph/prompts/`

## Validation and CI

> GNU Make >= 4 is required for project targets. On macOS, install with `brew install make` and use `gmake` unless your PATH already exposes GNU Make as `make`.

Use [`docs/guides/ci-strategy.md`](guides/ci-strategy.md) as the canonical validation guide.

Routine branch gate:

```bash
make agent-ci
```

Final ship/release gate:

```bash
make release-gate
```

Lower-level targets such as `ci-docs`, `ci-fast`, `ci`, and `macos-ci` still exist, but most contributors should treat them as internal tiers behind `make agent-ci` rather than commands to choose among day to day.
