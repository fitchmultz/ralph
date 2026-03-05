# macOS App

Purpose: document Ralph's macOS SwiftUI app, user-facing workflows, and CLI parity expectations.

## Overview

Ralph includes a macOS app for interactive queue and run supervision workflows.

The app is intended for:
- Browsing and editing `.ralph/queue.jsonc` and `.ralph/done.jsonc`
- Triage and prioritization with a richer visual layout than terminal output
- Triggering common run operations while keeping CLI-compatible behavior
- Multi-window workflows across repositories and workstreams

The app does **not** replace the CLI for automation, CI, or scripted workflows.

## Open the App

From a repository initialized with `ralph init`:

```bash
ralph app open
```

If you are not on macOS (or prefer terminal workflows), use the CLI directly:

```bash
ralph queue list
ralph run one
ralph run loop
```

## Feature Tour

The app centers around workspace navigation and fast task handling:

- **Queue**: inspect tasks, status, priority, and dependency context
- **Quick Actions**: shortcuts for frequent task and run operations
- **Run Control**: launch and supervise execution flows
- **Advanced Runner**: runner/model-oriented controls
- **Analytics**: high-level productivity and queue trend visibility
- **Graph View**: visualize dependency relationships
- **Command Palette**: keyboard-first command execution

## Keyboard Shortcuts

Documented from `apps/RalphMac/RalphMac/RalphMacApp.swift` command registrations.

### Navigation
- `⌘1`: Show Queue
- `⌘2`: Show Quick Actions
- `⌘3`: Show Run Control
- `⌘4`: Show Advanced Runner
- `⌘5`: Show Analytics
- `⌃⌘S`: Toggle sidebar
- `⇧⌘K`: Toggle view mode
- `⇧⌘G`: Show graph view

### Task Actions
- `⌥⌘N`: New task
- `⌘Return`: Start work on selected task

### Workspace / Window Management
- `⌘T`: New tab
- `⌘W`: Close tab
- `⇧⌘W`: Close window
- `⇧⌘]`: Next tab
- `⇧⌘[`: Previous tab
- `⌘D`: Duplicate tab

### Tools and Support
- `⌘K`: Quick command
- `⇧⌘P`: Command palette
- `⇧⌘L`: Export logs
- `⇧⌘R`: View crash reports
- `⌘,`: Settings

## How the App Integrates with the CLI

The app is a thin client that shells out to the `ralph` binary via `RalphCLIClient`.

Practical implications:
- CLI and app should remain behaviorally aligned for core task/run operations.
- Most data and execution issues can be reproduced via CLI commands.
- `ralph doctor` remains the primary diagnostics entry point.

## Automated UI Testing

UI automation exists and is intentionally separated from the default macOS CI path because UI tests are headed and can take over mouse/keyboard.
The Makefile now clears quarantine metadata and then re-signs UI test bundles ad-hoc before execution to avoid macOS Gatekeeper flagging `RalphMacUITests-Runner.app` as damaged.

Run all UI tests:

```bash
make macos-test-ui
# Shared workstation: RALPH_XCODE_JOBS=4 make macos-test-ui
# macOS/Homebrew GNU Make users: gmake macos-test-ui
```

Run UI tests with full visual artifact capture (timeline screenshots + exported attachments):

```bash
make macos-test-ui-artifacts
# writes timestamped artifacts under target/ui-artifacts/
# includes .xcresult bundle, exported attachments, and summary.txt
```

After reviewing visuals, clean artifacts to avoid disk growth:

```bash
make macos-ui-artifacts-clean
```

Run the focused window/tab shortcut regression suite:

```bash
make macos-test-window-shortcuts
# Shared workstation: RALPH_XCODE_JOBS=4 make macos-test-window-shortcuts
```

Test sources live in `apps/RalphMac/RalphMacUITests/`.

## Troubleshooting

### App does not open
- Verify you are on macOS.
- Run `ralph app open` from the repository root.

### Queue data not loading
- Confirm `.ralph/queue.jsonc` or `.ralph/queue.json` exists.
- Run `ralph queue validate` and resolve schema errors.

### Runner commands fail
- Run `ralph doctor` for environment diagnostics.
- Confirm configured runner CLIs are installed and authenticated.

### Need deterministic verification
- Validate behavior in terminal first (`ralph queue ...`, `ralph run ...`).
- Then verify equivalent flows in the app UI.

## Notes

- For complete command coverage and automation, use the CLI reference: `docs/cli.md`.
- For release-quality verification, run `make macos-ci` when app changes are in scope.
