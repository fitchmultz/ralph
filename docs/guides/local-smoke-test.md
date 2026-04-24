# Local Smoke Test (5-10 minutes)
Status: Active
Owner: Maintainers
Source of truth: this document for its stated scope
Parent: [Ralph Documentation](../index.md)


Purpose: provide a deterministic install and verification path without requiring external runner setup.

## Preconditions

- Fresh clone of repository
- Rust toolchain + GNU Make available

## Steps

```bash
# from repo root
make install
# macOS/Homebrew GNU Make users: gmake install

# initialize repo-local runtime state (safe to rerun)
ralph init

# verify command surface
ralph --help
ralph run one --help
ralph scan --help

# verify local repo state + diagnostics
ralph queue validate
ralph queue list
ralph doctor

# required quality gate
make agent-ci
```

For gate selection, macOS escalation, and resource-cap guidance, use [ci-strategy.md](ci-strategy.md).

If you want a shorter reviewer-oriented version of this flow, use [evaluator-path.md](evaluator-path.md).

## Expected Results

- help commands succeed
- queue validation/list commands succeed
- `ralph doctor` completes without critical failures in repo root
- `make agent-ci` passes
- source snapshots without `.git/` fall back to `make release-gate` (`macos-ci` on macOS with Xcode, otherwise `ci`)
- source snapshots must exclude local/runtime artifacts such as `target/`, unallowlisted `.ralph/*` content, repo-local env files (`.env`, `.env.*`, `.envrc` except `.env.example`), local notes (`.scratchpad.md`, `.FIX_TRACKING.md`), and `apps/RalphMac/build/`

## Troubleshooting

- GNU Make mismatch: use `gmake` on macOS Homebrew setups
- env/runtime artifact failures: run `make pre-public-check` for detailed diagnostics
- additional help: [docs/troubleshooting.md](../troubleshooting.md)
