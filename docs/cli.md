# CLI Reference
Status: Active
Owner: Maintainers
Source of truth: this document for its stated scope
Parent: [Ralph Documentation](index.md)


This page documents Ralph's current command surface. Default `ralph --help` shows the core workflow only; use `ralph help-all` or `ralph <command> --help` to reveal advanced and experimental surfaces.

## Global Flags

These are available on most commands:

- `--force`
- `-v, --verbose`
- `--debug` (supported on run flows; writes raw logs to `.ralph/logs/debug.log`)
- `--color <auto|always|never>`
- `--no-color`
- `--auto-fix`
- `--no-sanity-checks`

## Core Commands

- `ralph queue` - Inspect and manage queue/done files
- `ralph config` - Show resolved config, schema, paths, profiles, repo trust (`config trust init`)
- `ralph run` - Execute tasks (`one`, `loop`, `resume`, `parallel`)
- `ralph task` - Build/create and manage task lifecycle
- `ralph scan` - Create tasks by scanning repository state
- `ralph init` - Bootstrap `.ralph/` files
- `ralph app` - macOS app integration
- `ralph version` - Build/version info

## Advanced Commands

- `ralph machine` - Versioned machine-facing JSON API for the macOS app and automation
- `ralph prompt` - Render/export/sync/diff prompts
- `ralph doctor` - Environment diagnostics
- `ralph context` - Manage AGENTS.md context docs
- `ralph daemon` - Background daemon controls
- `ralph prd` - Convert PRD markdown into tasks
- `ralph completions` - Generate shell completions
- `ralph migrate` - Check/apply migrations
- `ralph cleanup` - Remove temporary runtime artifacts
- `ralph version` - Build/version info
- `ralph watch` - File watch to detect task comments
- `ralph webhook` - Test/status/replay webhook deliveries
- `ralph productivity` - Analytics summaries and trends
- `ralph plugin` - Plugin discovery and lifecycle
- `ralph runner` - Runner list and capabilities
- `ralph tutorial` - Interactive onboarding walkthrough
- `ralph undo` - Restore most recent queue snapshot

## Experimental Commands

- `ralph run parallel` - Experimental direct-push parallel worker operations

## High-Value Workflows

### Initialize

```bash
ralph init
ralph init --non-interactive
ralph init --trust-project-commands
```

When project config includes execution-sensitive settings (`agent.*_bin`, plugin runners, `agent.ci_gate`, `plugins.*`), create `.ralph/trust.jsonc` explicitly:

```bash
ralph config trust init
```

### Create and Run

```bash
ralph task "Stabilize flaky CI test"
ralph run one --profile safe
ralph run one --resume
ralph run one --debug
ralph run loop --max-tasks 0
ralph run loop --max-tasks 5
ralph run resume
```

`ralph run loop --max-tasks 0` means unlimited execution. Use a positive `--max-tasks` value when you want a fixed cap on successful iterations.

### Resume-aware execution

Ralph now explicitly narrates whether it is:
- resuming the same session
- falling back to a fresh invocation
- refusing to resume because confirmation is required

Useful commands:

```bash
# Inspect interrupted work and choose interactively when needed
ralph run one

# Auto-resume when Ralph can do so safely
ralph run one --resume
ralph run loop --resume --max-tasks 5
ralph run resume

# Headless use: explicit policy only
ralph run one --resume --non-interactive
ralph run loop --non-interactive
```

### Blocked / waiting / stalled narration

When Ralph cannot make progress, it now classifies the current state instead of only printing generic wait prose. Operator-facing run surfaces distinguish:

- true idle waiting (no todo work)
- dependency blocking
- schedule blocking
- queue lock contention
- CI-gate stalls
- runner/session recovery stalls

For automation, the same model is exposed through:

- `ralph machine run ...` NDJSON `blocked_state_changed` / `blocked_state_cleared` events
- `ralph machine run ...` terminal summaries via `blocking`
- `ralph machine queue read` via `runnability.summary.blocking`

### Execution Shape

```bash
# Single-pass
ralph run one --quick

# 2-phase
ralph run one --phases 2

# 3-phase (default)
ralph run one --phases 3
```

### Runner Overrides

```bash
ralph run one --runner codex --model gpt-5.4 --effort high
ralph run one --runner-phase1 codex --model-phase1 gpt-5.4 --effort-phase1 high
ralph run one --runner-phase2 codex --model-phase2 gpt-5.4 --effort-phase2 medium
```

### Queue Operations

```bash
ralph queue list
ralph queue next --with-title
ralph queue validate
ralph queue graph --format dot
ralph queue tree
ralph queue archive
```

### Task Lifecycle

```bash
ralph task build "Refactor queue parsing"
ralph task decompose "Build OAuth login with GitHub and Google"
ralph task decompose RQ-0001 --child-policy append --with-dependencies --write
ralph task decompose --attach-to RQ-0042 --format json "Plan webhook reliability work"
ralph task start RQ-0001
ralph task status doing RQ-0001
ralph task done RQ-0001 --note "Verified with make agent-ci"
```

On macOS, the app exposes the same workflow through `Decompose Task...` in the Task menu, command palette, queue toolbar, and task context menus.

### Diagnostics

```bash
ralph doctor
ralph doctor --format json
ralph runner list
ralph runner capabilities claude
ralph config show --format json
```

When Ralph is not making progress, `ralph doctor` now uses the same canonical `BlockingState` vocabulary as the live run surfaces: `waiting`, `blocked`, or `stalled`, with reasons such as `dependency_blocked`, `schedule_blocked`, `lock_blocked`, `ci_blocked`, `runner_recovery`, and `operator_recovery`.

### Recovery and continuation

```bash
ralph queue validate
ralph queue repair --dry-run
ralph queue repair
ralph task mutate --dry-run --input request.json
ralph task mutate --format json --input request.json
ralph task decompose --format json "Improve webhook reliability"
ralph task decompose --write "Improve webhook reliability"
ralph task followups apply --task RQ-0135
ralph task followups apply --task RQ-0135 --dry-run --format json
ralph undo --list
ralph undo --dry-run
```

These commands are now first-class continuation tools. They explain whether Ralph is ready, waiting, blocked, or stalled, preserve partial value where safe, and point to the next recovery step instead of treating queue repair or undo as emergency-only workflows.

If `ralph run loop` stops on queue validation, start with `ralph queue repair --dry-run` to preview recoverable fixes, apply them with `ralph queue repair`, and optionally confirm the result with `ralph queue validate`.

`ralph task mutate --format json` and `ralph task decompose --format json` now emit the same shared versioned continuation documents used by `ralph machine ...`.
`ralph task followups apply` consumes `.ralph/cache/followups/<TASK_ID>.json`, validates the proposal, creates undo, inserts generated tasks into the queue, and removes the proposal after a successful apply.

### Machine API

```bash
ralph machine system info
ralph machine queue read
ralph machine queue validate
ralph machine queue repair --dry-run
ralph machine queue undo --dry-run
ralph machine config resolve
ralph machine doctor report
ralph machine task mutate --input request.json
ralph machine run one --resume --id RQ-0001
ralph machine run loop --resume --max-tasks 5
ralph machine run loop --resume --max-tasks 0 --parallel 2
ralph machine run stop
ralph machine schema
```

Machine run loops use the same convention: `--max-tasks 0` means unlimited execution.

### Experimental Parallel Supervision

```bash
ralph run loop --parallel 4 --max-tasks 8
ralph run parallel status --json
ralph run parallel retry --task RQ-0007
```

Parallel direct-push execution is experimental. Keep it out of default onboarding paths and opt in only when the repository and branch policy are ready for it.

### Daemon and Watch

```bash
ralph daemon start
ralph daemon status
ralph daemon logs --follow --tail 200
ralph watch --auto-queue
```

### Webhooks

```bash
ralph webhook test
ralph webhook status
ralph webhook replay --dry-run --id <delivery-id>
```

### Prompt Management

```bash
ralph prompt list
ralph prompt worker --phase 1 --repo-prompt plan
ralph prompt export --all
ralph prompt diff worker
```

### Undo

```bash
ralph undo --list
ralph undo --dry-run
ralph undo
```

## Key Subcommand Groups

### `ralph run`

- `resume`
- `one`
- `loop`
- `parallel` (`status`, `retry`) - experimental

### `ralph queue`

- `validate`, `prune`, `next`, `next-id`, `show`, `list`, `search`, `archive`, `repair`, `unlock`, `sort`
- Analytics/reporting: `stats`, `history`, `burndown`, `aging`, `dashboard`
- Integrations: `schema`, `graph`, `export`, `import`, `issue`, `stop`, `explain`, `tree`

### `ralph task`

- Build/create: `task` (freeform), `build`, `refactor`, `build-refactor`, `from`, `template`, `followups`
- Lifecycle: `show`, `ready`, `status`, `done`, `reject`, `start`, `schedule`
- Editing: `field`, `edit`, `update`
- Structure/relations: `clone`, `split`, `relate`, `blocks`, `mark-duplicate`, `children`, `parent`
- Bulk operations: `batch`

## Shell Completions

```bash
ralph completions bash > ~/.local/share/bash-completion/completions/ralph
ralph completions zsh > ~/.zfunc/_ralph
ralph completions fish > ~/.config/fish/completions/ralph.fish
ralph completions powershell > $PROFILE.CurrentUserAllHosts
```

## Source of Truth

For behavior that may change between releases, trust live command help first:

```bash
ralph --help
ralph help-all
ralph <command> --help
ralph <command> <subcommand> --help
```
