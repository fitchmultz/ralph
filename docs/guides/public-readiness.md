# Public Readiness

Use this checklist before any public release window.

## Fast Safety Gate

Run the lightweight repo safety audit at least once while iterating:

```bash
make check-repo-safety
```

That delegates to:

```bash
scripts/pre-public-check.sh --skip-ci --skip-links --skip-clean
```

## Full Public-Readiness Audit

Before the real release mutation:

```bash
make pre-public-check
```

This runs:

- required public-file checks
- tracked runtime/build artifact checks
- `.env` tracking checks
- repo-wide secret-pattern scan
- repo-wide markdown link checks
- `make release-gate`

## Release-Context Audit

After `versioning.sh sync` has intentionally dirtied release metadata, use release-context mode instead of forcing a clean tree:

```bash
scripts/pre-public-check.sh --skip-ci --release-context
```

`--release-context` allows only the canonical release metadata paths to be dirty.

## Suggested Sequence

1. `make agent-ci`
2. `make pre-public-check`
3. `make release-verify VERSION=<x.y.z>`
4. `make release VERSION=<x.y.z>`

## Notes

- `agent-ci` now routes by dependency surface, not just `apps/RalphMac/` path prefixes.
- `make release-verify` is the canonical preflight for real releases and now prepares the exact local snapshot that `make release` publishes.
- Public-readiness scans the whole repo for markdown links and obvious secret patterns; do not rely on a short doc allowlist anymore.
