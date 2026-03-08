# CI and Test Strategy

Purpose: document which checks are required vs optional, and how Ralph controls runtime/resource usage during validation.

## Principles

- Keep default contributor checks fast and deterministic.
- Keep heavy/interactive checks opt-in and clearly labeled.
- Bound parallelism by default so local checks do not monopolize developer machines.

## Required for Day-to-Day Development (PR-equivalent)

Run:

```bash
make agent-ci
```

Behavior:

- Always runs fast Rust/CLI gate for non-app changes.
- Auto-escalates to macOS gate when paths under `apps/RalphMac/` change.

Fast Rust/CLI gate is `make ci-fast`:

- `check-env-safety` (delegates to pre-public safety checks: required-files + secret/runtime/env validation)
- `check-backup-artifacts`
- `deps`
- `format`
- `type-check`
- `lint`
- `test`

## Full Rust Release Gate

Run:

```bash
make ci
```

Includes `ci-fast` plus release-shape checks:

- release build
- schema generation
- install verification

Use this before release tagging and public-readiness checks.

## macOS Ship Gate (App in Scope)

Run:

```bash
make macos-ci
```

Includes:

- `ci` (full Rust gate)
- macOS app build
- macOS app non-UI tests

UI automation is intentionally excluded from `macos-ci` by default.

## Canonical Release Gate

Run:

```bash
make release-gate
```

Behavior:

- runs `macos-ci` on macOS when Xcode is available
- otherwise runs `ci`
- is the shared gate used by `make release-verify` and `scripts/pre-public-check.sh`

## Heavy / Interactive / On-Demand Checks

Run only when needed (manual or scheduled in your own automation):

```bash
make macos-test-ui
make macos-test-ui-artifacts
make macos-test-window-shortcuts
make coverage
```

Why on-demand:

- UI tests are headed and can take over keyboard/mouse.
- Visual artifact capture is intentionally opt-in and can generate large files.
- Coverage and UI suites are materially heavier than core gates.

After reviewing visual evidence from `make macos-test-ui-artifacts`, clean up to control disk usage:

```bash
make macos-ui-artifacts-clean
```

## Resource Controls

Ralph’s make targets support resource caps:

```bash
RALPH_CI_JOBS=4 make agent-ci
RALPH_XCODE_JOBS=4 make macos-test-window-shortcuts
RALPH_CI_JOBS=4 RALPH_XCODE_JOBS=4 make pre-public-check
```

Defaults:

- `RALPH_CI_JOBS=4` limits cargo/nextest concurrency.
- `RALPH_XCODE_JOBS=4` limits xcodebuild concurrency.
- Set either value to `0` to use tool defaults.

## Suggested Cadence

- On every branch update: `make agent-ci`
- Before merge to release-ready branch: `make ci`
- For app-heavy changes: `make macos-ci`
- Overnight/manual quality sweep: UI tests + coverage

## Expected Runtime Profile (guidance)

Actual times vary by machine and cache warmth.

- `make agent-ci` should be the fastest stable gate.
- `make ci` is heavier due to release build/schema/install steps.
- `make macos-ci` is heaviest among non-UI defaults.
- UI suites and coverage are intentionally separated to protect everyday DX.
