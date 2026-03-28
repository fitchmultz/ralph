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

- Routes docs/community-only changes to `make ci-docs`.
- Routes non-app executable changes to `make ci-fast`.
- Auto-escalates to macOS gate when the changed dependency surface can affect the bundled app contract (CLI/runtime/config/build/app paths).

Docs/community-only gate is `make ci-docs`:

- `check-env-safety` (delegates to pre-public safety checks: required-files + secret/runtime/env validation)
- `check-backup-artifacts`

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
- deterministic macOS contract smoke (`make macos-test-contracts`, currently the Settings open-path + workspace-routing contracts)

Interactive XCTest UI automation is intentionally excluded from `macos-ci` by default.

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
- Visual artifact capture is intentionally opt-in and preserves `RalphMacUITests.xcresult` plus `summary.txt` for later review.
- Coverage and UI suites are materially heavier than core gates.

After reviewing the preserved `.xcresult` bundle and summary from `make macos-test-ui-artifacts`, clean up to control disk usage:

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

- `RALPH_CI_JOBS=0` lets cargo/nextest use tool-managed parallelism for fastest local iteration.
- `RALPH_XCODE_JOBS=4` keeps macOS app builds friendly by default.
- Set either value explicitly (for example `RALPH_CI_JOBS=4`) on shared workstations.

## Suggested Cadence

- On every branch update: `make agent-ci`
- Before merge to release-ready branch: `make ci`
- For app-heavy changes: `make macos-ci`
- Overnight/manual quality sweep: UI tests + coverage

## Headless Profiling Loop

When CI speed regresses, capture the same local evidence before and after changes:

```bash
time make agent-ci
NEXTEST_EXPERIMENTAL_LIBTEST_JSON=1 cargo nextest run --workspace --all-targets --locked --show-progress none --status-level none --final-status-level none --message-format libtest-json-plus > target/profiling/nextest.jsonl
cargo test --workspace --doc --locked -- --include-ignored
```

Prefer headless paths first; interactive UI automation remains opt-in and out of the default gate.

### Targeted Suite Profiling

When cutting over a known slow integration target, avoid another workspace-wide sweep. Profile the exact suite before and after each focused change:

```bash
mkdir -p target/profiling

NEXTEST_EXPERIMENTAL_LIBTEST_JSON=1 \
cargo nextest run --workspace --locked --test run_parallel_test \
  --show-progress none \
  --status-level none \
  --final-status-level none \
  --message-format libtest-json-plus \
  > target/profiling/nextest.run_parallel_test.before.jsonl

# apply one focused fixture/locking change

NEXTEST_EXPERIMENTAL_LIBTEST_JSON=1 \
cargo nextest run --workspace --locked --test run_parallel_test \
  --show-progress none \
  --status-level none \
  --final-status-level none \
  --message-format libtest-json-plus \
  > target/profiling/nextest.run_parallel_test.after.jsonl
```

Use the same pattern for `parallel_direct_push_test` and any same-pattern follow-on suite.

Optimization rules for Rust integration tests:
- Hold `env_lock()` only when mutating `PATH` or other process-global env vars.
- If a fake runner is configured via an explicit `*_bin` path in `.ralph/config.jsonc`, do not also mutate `PATH`.
- Prefer `seed_ralph_dir()` over `ralph_init()` when the test only needs cached `.ralph/` scaffolding and is not asserting real init behavior.

## Expected Runtime Profile (guidance)

Actual times vary by machine and cache warmth.

- `make agent-ci` should be the fastest stable gate for the current change surface.
- `make ci` is heavier due to release build/schema/install steps.
- `make macos-ci` is heaviest among non-UI defaults.
- UI suites and coverage are intentionally separated to protect everyday DX.
