# CLI Reference

This page documents Ralph's current command surface. For full option details, use `ralph <command> --help`.

## Global Flags

These are available on most commands:

- `--force`
- `-v, --verbose`
- `--debug` (supported on run flows; writes raw logs to `.ralph/logs/debug.log`)
- `--color <auto|always|never>`
- `--no-color`
- `--auto-fix`
- `--no-sanity-checks`

## Top-Level Commands

- `ralph queue` - Inspect and manage queue/done files
- `ralph config` - Show resolved config, schema, paths, profiles
- `ralph run` - Execute tasks (`one`, `loop`, `resume`, `parallel`)
- `ralph task` - Build/create and manage task lifecycle
- `ralph scan` - Create tasks by scanning repository state
- `ralph init` - Bootstrap `.ralph/` files
- `ralph app` - macOS app integration
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

## High-Value Workflows

### Initialize

```bash
ralph init
ralph init --non-interactive
```

### Create and Run

```bash
ralph task "Stabilize flaky CI test"
ralph run one
ralph run one --debug
ralph run loop --max-tasks 5
```

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
ralph runner list
ralph runner capabilities claude
ralph config show --format json
```

### Parallel Supervision

```bash
ralph run loop --parallel 4 --max-tasks 8
ralph run parallel status --json
ralph run parallel retry --task RQ-0007
```

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
- `parallel` (`status`, `retry`)

### `ralph queue`

- `validate`, `prune`, `next`, `next-id`, `show`, `list`, `search`, `archive`, `repair`, `unlock`, `sort`
- Analytics/reporting: `stats`, `history`, `burndown`, `aging`, `dashboard`
- Integrations: `schema`, `graph`, `export`, `import`, `issue`, `stop`, `explain`, `tree`

### `ralph task`

- Build/create: `task` (freeform), `build`, `refactor`, `build-refactor`, `from`, `template`
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
ralph <command> --help
ralph <command> <subcommand> --help
```
