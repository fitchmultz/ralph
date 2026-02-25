# Quick Start Guide

Get Ralph running in a repository in a few minutes.

## 1) Install

```bash
cargo install ralph
```

Or from source:

```bash
git clone https://github.com/mitchfultz/ralph
cd ralph
make install
```

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

Each also supports a `.json` fallback.

## Next Docs

- [CLI Reference](cli.md)
- [Configuration](configuration.md)
- [Queue and Tasks](queue-and-tasks.md)
