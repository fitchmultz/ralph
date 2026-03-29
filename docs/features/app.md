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

Security note:
- `ralph app open` deep links now carry only workspace context.
- The app ignores legacy `cli=` URL parameters and will not swap the CLI executable from URL input.

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
- `⌥⌘D`: Decompose task
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

## Task Decomposition

The macOS app now exposes the same preview-first decomposition workflow as the CLI.

Use any of these entry points:
- Task menu: `Decompose Task...`
- Command palette: `Decompose Task...`
- Queue toolbar: `Decompose`
- Queue row context menu: `Decompose Task...`
- Menu bar: `Decompose Task...`

Behavioral notes:
- The sheet defaults to preview mode and only writes after an explicit second action.
- Launching from a selected task defaults to decomposing that task in place.
- Freeform mode can optionally attach a new subtree under an existing parent.
- The app calls `ralph task decompose --format json` and renders the stable CLI response; it does not reimplement planner logic locally.

## How the App Integrates with the CLI

The app is a thin client that shells out to the `ralph` binary via `RalphCLIClient`.

Practical implications:
- CLI and app should remain behaviorally aligned for core task/run operations.
- Most data and execution issues can be reproduced via CLI commands.
- `ralph doctor` remains the primary diagnostics entry point.

## Automated UI Testing

UI automation exists and is intentionally separated from the default macOS CI path because UI tests are headed and can take over mouse/keyboard.
The Makefile clears quarantine metadata and re-signs UI test bundles ad-hoc to avoid macOS Gatekeeper flagging `RalphMacUITests-Runner.app` as damaged.
Because macOS may require a one-time password/Touch ID approval when a rebuilt bundle first requests UI automation, the local workflow is split into build-once and retest-only targets.

Build/sign UI bundles once for an interactive debugging session:

```bash
make macos-ui-build-for-testing
```

Re-run UI tests without rebuilding bundles:

```bash
make macos-ui-retest
# Focus one test:
RALPH_UI_ONLY_TESTING=RalphMacUITests/RalphMacUILaunchAndTaskFlowTests/test_createNewTask_viaQuickCreate make macos-ui-retest
```

Run all UI tests end-to-end in one command:

```bash
make macos-test-ui
# macOS/Homebrew GNU Make users: gmake macos-test-ui
```

Run the focused window/tab shortcut regression suite:

```bash
make macos-test-window-shortcuts
```

For shared-workstation caps and preserved `.xcresult` capture with `make macos-test-ui-artifacts`, use `docs/guides/ci-strategy.md`.

Test sources live in `apps/RalphMac/RalphMacUITests/`.

## Troubleshooting

### App does not open
- Verify you are on macOS.
- Run `ralph app open` from the repository root.

### Queue data not loading
- Confirm `.ralph/queue.jsonc` exists.
- Run `ralph queue validate` and resolve schema errors.

### Runner commands fail
- Run `ralph doctor` for environment diagnostics.
- Confirm configured runner CLIs are installed and authenticated.

### Need deterministic verification
- Validate behavior in terminal first (`ralph queue ...`, `ralph run ...`).
- Then verify equivalent flows in the app UI.

## Notes

- For complete command coverage and automation, use the CLI reference: `docs/cli.md`.
- For release-quality verification, run `make macos-ci` when app changes are in scope; it now includes deterministic noninteractive Settings + workspace-routing contract coverage alongside the standard app build/tests.
