# Public Readiness Checklist

Purpose: provide a repeatable audit before making the repository public.

Use this checklist before each public release window.

## 1) Repository Hygiene

- [ ] Ensure local machine artifacts are ignored (`apps/RalphMac/build/`, `.DS_Store`, Xcode user data).
- [ ] Ensure no secrets are tracked:
  - `git grep -nE '(API_KEY|SECRET|TOKEN|PASSWORD)' -- ':!docs/'`
- [ ] Ensure no temporary scratch files are tracked (`.scratchpad.md`, backup files).
- [ ] Ensure working tree is clean before tagging/public push.

## 2) Documentation Quality

- [ ] `README.md` explains what Ralph is, why it exists, and how to run it quickly.
- [ ] README links to: docs index, contributing, security, changelog, portfolio guide.
- [ ] Architecture diagram in README still matches current runtime behavior.
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
```

## 4) Code Quality and Test Health

- [ ] Local CI passes from a clean tree:

```bash
make agent-ci
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
- [ ] Before public launch, optionally squash/fixup noisy WIP commits:

```bash
git log --oneline -n 40
git rebase -i HEAD~20
```

- [ ] Ensure commit messages explain user-visible behavior changes (not only implementation details).

## 6) Release and Community Metadata

- [ ] `CHANGELOG.md` has a meaningful Unreleased section.
- [ ] `.github` templates exist and are current (issues + PR template).
- [ ] `SECURITY.md` reporting instructions are accurate.
- [ ] `CODE_OF_CONDUCT.md` and `CONTRIBUTING.md` are linked and up to date.

## 7) Final Pre-Public Pass

Run:

```bash
git status --short
make agent-ci
```

If all checks pass, perform final review of README + PORTFOLIO guide, then publish.
