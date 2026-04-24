# Quick Start Guide
Status: Active
Owner: Maintainers
Source of truth: this document for its stated scope
Parent: [Ralph Documentation](index.md)


Get Ralph running in a repository in a few minutes.

## 1) Install

```bash
cargo install ralph-agent-loop
```

This installs the `ralph` executable.

Or from source:

```bash
git clone https://github.com/fitchmultz/ralph
cd ralph
make install
```

> macOS note: install GNU Make with `brew install make` and use `gmake ...` unless your PATH already points `make` to Homebrew gnubin.

## 2) Initialize a Repository

```bash
cd your-project
ralph init
```

Non-interactive setup (CI/scripts):

```bash
ralph init --non-interactive
```

## 3) Create Tasks

```bash
# Freeform task creation
ralph task "Add regression tests for queue repair"

# Or use task builder explicitly
ralph task build "Audit webhook retry behavior"
```

## 4) Run Tasks

```bash
# Run one runnable task
ralph run one

# Run continuously until queue is drained
ralph run loop
```

Useful run variants:

```bash
# Single-pass mode
ralph run one --quick

# Explicit 3-phase supervision mode
ralph run one --phases 3

# Dry-run selection only (no execution)
ralph run one --dry-run
```

If you have not configured a runner yet, stop at `--dry-run` and use the local smoke test instead of a real execution pass.

Useful readiness checks before a real run:

```bash
ralph runner list
ralph runner capabilities claude
ralph doctor
```

## 5) Inspect Queue State

```bash
ralph queue list
ralph queue next --with-title
ralph queue validate
```

## 6) Verify Environment

```bash
ralph doctor
ralph runner list
ralph runner capabilities claude
```

## 7) Optional Automation

```bash
# Background worker process
ralph daemon start

# Watch source files for TODO/FIXME/HACK/XXX and create tasks
ralph watch --auto-queue
```

## 8) macOS App

```bash
ralph app open
```

## Where Files Live

Default runtime files:

- `.ralph/queue.jsonc`
- `.ralph/done.jsonc`
- `.ralph/config.jsonc`

This repository intentionally keeps a sanitized `.ralph/` state for dogfooding and reproducible demos. In your own projects, treat `.ralph/` as project-local runtime state.

## Next Docs

- [Evaluator Path](guides/evaluator-path.md)
- [Local Smoke Test](guides/local-smoke-test.md)
- [CLI Reference](cli.md)
- [Configuration](configuration.md)
- [Queue and Tasks](queue-and-tasks.md)
