# Release Runbook

Purpose: provide the shortest safe path to ship a Ralph release with the transaction workflow.

## Preconditions

- Working tree is clean.
- `CHANGELOG.md` has release-worthy notes under `## [Unreleased]`.
- `gh auth status` succeeds.
- crates.io publish credentials are available.
- `rustc --version` matches `rust-toolchain.toml`, or your shell is using the pinned rustup toolchain.

## Recommended Flow

1. Run preflight:

```bash
make release-verify VERSION=<version>
```

2. If preflight passes, execute the release:

```bash
make release VERSION=<version>
```

3. If a remote publish step fails after local preparation, reconcile the recorded transaction:

```bash
scripts/release.sh reconcile <version>
```

## What `make release-verify` Does

- syncs version metadata
- checks version drift
- runs `scripts/pre-public-check.sh --skip-ci --release-context`
- runs `make release-gate`
- runs `scripts/release.sh verify <version>`

## What `make release` Does

- runs `scripts/release.sh execute <version>`
- prepares the full local release state before remote publication
- records transaction state under `target/release-transactions/v<version>/state.env`

## Evidence to Capture

- `make release-verify VERSION=<version>` output
- final `git status --short`
- `gh release view v<version>`
- the transaction state path if reconcile was required

## Notes

- `Cargo.lock` is release metadata, not incidental noise.
- `target/release-artifacts/` is disposable output owned by the release scripts.
- `scripts/release.sh reconcile <version>` is the only supported continuation path after a partial remote failure.
