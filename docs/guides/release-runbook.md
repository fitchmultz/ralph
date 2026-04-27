# Release Runbook
Status: Active
Owner: Maintainers
Source of truth: this document for its stated scope
Parent: [Ralph Documentation](../index.md)


Purpose: provide the shortest safe path to ship a Ralph release with the transaction workflow.

## Preconditions

- Working tree is clean.
- `CHANGELOG.md` has release-worthy notes under `## [Unreleased]`.
- `gh auth status` succeeds.
- crates.io publish credentials are available.
- `make rust-toolchain-drift-check` passes, proving the repo-pinned Rust baseline still matches global rustup stable for this release window.

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

- runs `scripts/release.sh verify <version>`
- prepares the publish-ready local snapshot (`VERSION`, changelog, app metadata, artifacts, release notes)
- records verification state under `target/release-verifications/v<version>/`

## What `make release` Does

- runs `scripts/release.sh execute <version>`
- validates the recorded verification snapshot still matches `HEAD` and the local files
- creates the release commit/tag, pushes `main` + `v<version>`, prepares a GitHub draft release, publishes crates.io, then publishes the GitHub release
- records transaction state under `target/release-transactions/v<version>/state.env`

## Evidence to Capture

- `make rust-toolchain-drift-check` output, or the embedded `release-gate` output from `make release-verify VERSION=<version>`
- `make release-verify VERSION=<version>` output
- final `git status --short`
- `gh release view v<version>`
- the transaction state path if reconcile was required

## Notes

- `Cargo.lock` is release metadata, not incidental noise.
- Rust source-build baseline drift is handled by `make rust-toolchain-drift-check`; intentional adoption updates `rust-toolchain.toml` and crate `rust-version` together, not `scripts/versioning.sh sync` alone.
- A successful `make release-verify` intentionally leaves release metadata dirty until `make release` turns it into the release commit.
- `target/release-artifacts/` is disposable output owned by the release scripts.
- `scripts/release.sh reconcile <version>` is the only supported continuation path after a partial remote failure.
- Failure before crates.io publication is still in the reversible phase of the transaction; finish or roll back the pushed tag/draft release before retrying broad announcement.
- Failure after crates.io publication is urgent completion work, not a casual "resume later" state, because the irreversible cutover already happened.
