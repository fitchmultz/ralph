# Evaluator Path
Status: Active
Owner: Maintainers
Source of truth: this document for its stated scope
Parent: [Ralph Documentation](../index.md)


Use this guide when you want a fast, high-signal evaluation of Ralph without first wiring up an external runner.

## Goal

Validate four things quickly:

1. the CLI installs and runs cleanly
2. the repository-local task queue model is easy to inspect
3. the docs point to a sane workflow
4. the local quality gate is green

## Fastest Path

From a fresh clone:

```bash
# install locally from source
make install
# macOS/Homebrew GNU Make users: gmake install

# initialize repo-local runtime files
ralph init

# inspect the command surface
ralph --help
ralph queue list
ralph queue graph
ralph doctor

# run the required local gate
make agent-ci
```

If you prefer the full step-by-step version, use [local-smoke-test.md](local-smoke-test.md).

## What To Look For

- `ralph --help` should make the main command groups easy to discover.
- `ralph queue list` and `ralph queue graph` should show the repo's structured queue model without any remote setup.
- `ralph doctor` should explain environment readiness in plain language.
- `make agent-ci` should give you confidence that local verification is the real gate.
- On source snapshots without `.git/`, `make agent-ci` should fall back to `make release-gate` instead of forcing the macOS-only path.
- That source snapshot still needs to be export-clean: `target/`, unallowlisted `.ralph/*` content, repo-local env files (`.env`, `.env.*`, `.envrc` except `.env.example`), local notes (`.scratchpad.md`, `.FIX_TRACKING.md`), and app build outputs should be absent.

## If You Want One Real Workflow

After the basic smoke test, try one lightweight end-to-end repo-local flow:

```bash
ralph task "Document the evaluator quick path"
ralph queue list
ralph queue show RQ-0001
ralph run one --dry-run
```

That demonstrates task creation, queue inspection, and runnable-task selection without requiring a configured model runner.

## If You Want Runner-Aware Validation

Only after the smoke test:

```bash
ralph runner list
ralph runner capabilities claude
ralph run one --phases 3
```

Use that path if you specifically want to evaluate supervised execution rather than repo-local ergonomics.
