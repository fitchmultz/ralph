# Troubleshooting
Status: Active
Owner: Maintainers
Source of truth: this document for its stated scope
Parent: [Ralph Documentation](index.md)


Purpose: provide fast resolution paths for common setup and CI failures.

## GNU Make Errors on macOS

Symptom: Makefile errors about GNU Make version.

Fix:

```bash
brew install make
gmake agent-ci
```

## `make agent-ci` Fails on Env Safety

Symptom: tracked env file detected.

Fix:

```bash
git rm --cached <env-file>
# keep only .env.example tracked
```

## `make pre-public-check` Fails on Runtime Artifacts

Symptom: tracked `.ralph/...` runtime paths or build outputs detected.

Fix:

```bash
git rm --cached -r apps/RalphMac/build .ralph/cache .ralph/logs .ralph/lock .ralph/workspaces .ralph/undo .ralph/webhooks
```

Then rerun:

```bash
make pre-public-check
```

## Test Failures in Temporary Directory Logic

Symptom: flaky integration tests around temp paths or queue fixtures.

Fixes:

- ensure `ralph init --non-interactive` is used in tests
- rerun with `make test` to use the project harness
- inspect `crates/ralph/tests/test_support.rs` helpers for deterministic setup

## macOS App Build/Test Failures

Symptom: xcodebuild failures or UI test runner signing/quarantine issues.

Fixes:

```bash
make macos-build
make macos-test
# for interactive UI runs only
make macos-ui-build-for-testing
make macos-ui-retest
```

If macOS prompts for password/Touch ID before a UI run, that is the system approving Accessibility/Automation for a rebuilt test bundle. Reduce repeated prompts by building once and then iterating with `make macos-ui-retest` instead of rebuilding every run.

If an interrupted run strands `target/tmp/locks/xcodebuild.lock`, rerun the same target. Ralph now removes stale project-owned Xcode build locks automatically once the recorded owner PID is gone, and it keeps waiting only for live holders.

For gate choice, shared-workstation caps, and preserved UI evidence capture, use [`docs/guides/ci-strategy.md`](guides/ci-strategy.md).

## Need Visual Evidence from UI Tests

Symptom: UI run appears noisy/flaky but tests still pass, and you need inspectable visuals.

Use `make macos-test-ui-artifacts` for preserved `.xcresult` output, or use `RALPH_UI_ONLY_TESTING=... make macos-ui-retest` for focused reruns. Keep the full workflow in [`docs/guides/ci-strategy.md`](guides/ci-strategy.md).
