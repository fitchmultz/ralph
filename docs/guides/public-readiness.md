# Public Readiness Checklist

Purpose: provide a repeatable audit before making the repository public.

Use this checklist before each public release window.

## 1) Repository Hygiene

- [ ] Ensure local machine artifacts are ignored (`apps/RalphMac/build/`, `.DS_Store`, Xcode user data).
- [ ] Run best-effort secret-pattern scan:
  - `scripts/pre-public-check.sh --skip-ci --skip-links --skip-clean`
- [ ] Ensure no temporary scratch files are tracked (`.scratchpad.md`, backup files).
- [ ] Run fast safety gate:
  - `make check-repo-safety`
- [ ] Ensure working tree is clean before tagging/public push.

## 2) Documentation Quality

- [ ] `README.md` explains what Ralph is, why it matters for teams using AI coding agents, and how to run it quickly.
- [ ] `README.md` includes one concrete end-to-end repo workflow from request → queued task → run → verification.
- [ ] README links to: docs index, contributing, security, changelog, and local smoke test.
- [ ] Architecture diagram in README still matches current runtime behavior.
- [ ] `docs/guides/local-smoke-test.md` still matches the current install and verification flow.
- [ ] `docs/cli.md` matches current `--help` outputs for changed commands.
- [ ] `docs/features/app.md` reflects current macOS app capabilities and shortcuts.

## 3) UI/UX Usability Signals (macOS app)

- [ ] Core flow works in one pass: open repo → inspect queue → create task → start work.
- [ ] Keyboard shortcuts documented in `docs/features/app.md` still function.
- [ ] Error paths are understandable (missing CLI binary, queue parse issues, permissions).
- [ ] Window/tab commands behave predictably in multi-window scenarios.

Suggested local checks:

```bash
make macos-build
make macos-test
make macos-test-window-shortcuts
# Shared workstation: RALPH_XCODE_JOBS=4 make macos-test-window-shortcuts
```

## 4) Code Quality and Test Health

- [ ] Local PR-equivalent gate passes from a clean tree:

```bash
make agent-ci
```

- [ ] Full Rust release gate passes before tagging/public release:

```bash
make ci
```

- [ ] If app behavior changed, run ship gate:

```bash
make macos-ci
```

- [ ] No warnings are introduced in Rust (`clippy -D warnings`) or Xcode builds.
- [ ] New behavior has tests (unit/integration/snapshot as appropriate).

## 5) Commit History and Reviewer Experience

- [ ] Recent commits are understandable and logically grouped.
- [ ] Commit subjects follow project conventions (`RQ-####: summary`) when applicable.
- [ ] Ensure commit messages explain user-visible behavior changes (not only implementation details).
- [ ] If recent history is noisy, prefer a few explicit cleanup commits over leaving confusing public-facing drift.

## 6) Release and Community Metadata

- [ ] `CHANGELOG.md` has a meaningful Unreleased section.
- [ ] `.github` templates exist and are current (issues + PR template).
- [ ] `SECURITY.md` reporting instructions are accurate.
- [ ] `CODE_OF_CONDUCT.md` and `CONTRIBUTING.md` are linked and up to date.
- [ ] Screenshots and demo assets still look current, intentional, and match the shipped UI/CLI terminology.

## 7) Final Pre-Public Pass

Run the automated audit (includes required-file checks, runtime-artifact checks, key markdown-link checks, and CI):

```bash
make pre-public-check
# Shared workstation: RALPH_CI_JOBS=4 RALPH_XCODE_JOBS=4 make pre-public-check
```

Or invoke the script directly when you want to skip selected checks:

```bash
scripts/pre-public-check.sh --help
scripts/pre-public-check.sh --skip-clean --skip-ci
# fast safety-only gate
make check-repo-safety
```

If all checks pass, perform a final review of README, screenshots/demo assets, and the tracked `.ralph/` sample state, then publish.

Release-specific reminders:

- Run the public-readiness audit before the final release mutation so the working tree is still clean.
- `Cargo.lock` is expected release metadata when `VERSION` changes; review it, do not discard it.
- `target/release-artifacts/` is disposable build output owned by `scripts/release.sh`.

Related references:
- [CI and Test Strategy](ci-strategy.md)
