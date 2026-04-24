# RalphMac Agent Guide

<!-- AGENTS ONLY: app-scoped guidance. Repo-wide rules live in ../AGENTS.md. -->

Status: Active
Owner: Maintainers
Source of truth: this file for app-scoped guidance only; `../AGENTS.md` for
repo-wide rules.
Parent: `../AGENTS.md`

RalphMac is the native SwiftUI app for Ralph. It must stay a thin, user-friendly
surface over the Rust CLI and versioned `ralph machine ...` JSON contracts.

## Canonical App Rules

- Use `../AGENTS.md` for repository-wide CI, release, source-of-truth, cutover,
  and documentation rules. Do not duplicate those rules here.
- Native app workflows must use versioned machine contracts or shared JSON
  command outputs. Do not parse human CLI output.
- The Advanced Runner is diagnostic/debug tooling only. A CLI command is not
  app-parity-complete just because it can be run there.
- App-side task editing should use the canonical task mutation path rather than
  field-by-field shellouts.
- App behavior must preserve CLI parity while improving UX: guided inputs,
  visible state, previews where supported, progress, success/error states,
  recovery actions, keyboard access, and clear labels.
- If a CLI feature changes, update the machine contract, RalphCore decoding,
  native SwiftUI surface, tests, and docs together or record the explicit block
  in the parity registry (`../crates/ralph/src/cli/app_parity.rs`).

## Security Boundaries

- RalphMac may execute only the validated Ralph CLI path selected by the app or
  user. Do not add alternate executable discovery paths, shell-string execution,
  or GUI-controlled arbitrary command launch surfaces.
- Workspace access is user-selected and workspace-scoped. Do not broaden file
  reads/writes outside the active workspace except for documented Ralph config,
  cache, or app-support locations.
- The GUI does not make direct network calls for Ralph workflows. Networked
  behavior must stay behind the CLI/machine contract or a documented app system
  service with explicit owner approval.
- Keep destructive or trust-changing operations visible and confirmed in the UI;
  do not add hidden bypasses around CLI trust, config, or queue safeguards.

## Canonical Build and Test Entry Points

Derived summary; `../docs/guides/ci-strategy.md` is the validation source of
truth. Use Make targets from the repository root. They wrap Xcode with the
required bundling, locks, derived-data policy, and deterministic smoke tests.

| Need | Command |
|------|---------|
| Required app ship gate | `make macos-ci` |
| Routed local gate | `make agent-ci` |
| Build app | `make macos-build` |
| Non-UI app tests | `make macos-test` |
| Build UI-test bundles once | `make macos-ui-build-for-testing` |
| Re-run UI tests | `make macos-ui-retest` |
| Capture headed UI artifacts | `make macos-test-ui-artifacts` |
| Clean UI artifacts | `make macos-ui-artifacts-clean` |

`scripts/ralph-cli-bundle.sh` is the only CLI bundling/build entrypoint for
Makefile, Xcode, and release consumers. Do not add standalone Cargo fallback
logic inside Xcode project settings or app-specific scripts.

## App Architecture Boundaries

- `RalphMacApp.swift` stays a thin shell. Menu commands live in
  `RalphMacCommands.swift`, URL routing in `RalphMacApp+URLRouting.swift`,
  bootstrap helpers in `RalphMacApp+Support.swift`, window root composition in
  `WindowViewContainer.swift`, and UI-test window policy in
  `WorkspaceWindowAnchor.swift`.
- `Workspace.swift` is a `@MainActor` facade over domain owners and focused
  `Workspace+...` files. Do not re-accumulate runner, persistence, task
  mutation, recovery, or queue refresh logic in the root type.
- `RalphCLIClient.swift` owns the core subprocess API only. Retry helpers,
  recovery classification, health probing, and process lifecycle ownership live
  in their dedicated companion files.
- `RalphModels.swift` is a facade only. Keep CLI spec models, generic JSON
  values, argument-building helpers, and task-domain models in dedicated
  RalphCore files.
- Task list/detail views should stay orchestration-focused. Put reusable
  sections in `TaskListSections.swift` / `TaskDetailSections.swift` and dense
  transient state in dedicated state owners.

## App State and Communication

- Active-window navigation/task commands flow through focused scene values
  (`WorkspaceUIActions` / `WorkspaceWindowActions`).
- Unfocused menu, URL, and app lifecycle surfaces route through
  `WorkspaceSceneRouter`.
- Do not reintroduce process-wide `NotificationCenter` broadcasts for focused
  workspace actions.
- Queue file watcher refreshes and CLI queue JSON decoding use
  `WorkspaceQueueSnapshotLoader`; publish only final state on the main actor.
- Operational visibility flows through `WorkspaceOperationalHealth` so watcher,
  persistence, crash-report, and CLI health share one severity model.
- Workspace identity persistence uses a single `.snapshot` payload per
  workspace through `WorkspaceStateStore`; persistence failures must surface as
  `PersistenceIssue`.

## Testing Conventions

- Use `RalphCoreTestSupport` for temp workspaces, readiness polling, and cleanup
  assertions.
- SwiftUI previews that need workspace URLs derive them from
  `PreviewWorkspaceSupport`.
- UI tests must not write into the production app defaults domain. `--uitesting`
  launches use the dedicated test suite.
- Normal UI-test launches should keep one visible workspace window; multiwindow
  tests should keep two. Avoid widths below the split-view practical minimum.
- UI screenshot capture is opt-in through `RALPH_UI_SCREENSHOTS=1` or
  `RALPH_UI_SCREENSHOT_MODE`.

## File Headers and Style

Every Swift file starts with a purpose header covering responsibilities,
non-scope, and invariants/assumptions. Keep access control minimal and explicit:
use `private` for implementation details, internal by default, and `public` only
for real RalphCore framework exports.
