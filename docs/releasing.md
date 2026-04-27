# Releasing Ralph
Status: Active
Owner: Maintainers
Source of truth: this document for its stated scope
Parent: [Ralph Documentation](index.md)


Ralph releases now use an explicit verify-then-publish transaction model. `verify` prepares a publish-ready local snapshot, records verification state under `target/release-verifications/`, and `execute` publishes only if that exact snapshot still matches the workspace.

## Canonical Commands

```bash
# Recommended preflight
make release-verify VERSION=0.2.0

# Real release
make release VERSION=0.2.0

# Inspect or continue a release transaction
scripts/release.sh verify 0.2.0
scripts/release.sh reconcile 0.2.0
```

## Transaction Model

`scripts/release.sh` has explicit transaction commands:

| Invocation | Purpose |
| --- | --- |
| `scripts/release.sh verify <version>` | Prepare and record a publish-ready local snapshot without remote publication |
| `scripts/release.sh execute <version>` | Validate the recorded snapshot, then publish through the transaction pipeline |
| `scripts/release.sh reconcile <version>` | Resume the recorded transaction at its next incomplete phase |

The full release flow now runs in this order:

1. `verify` checks prerequisites, repo state, and release-note/changelog contract.
2. `verify` syncs version metadata from `VERSION`.
3. `verify` preserves curated `Unreleased` changelog notes when present, or auto-generates entries when the section is blank, then promotes the section.
4. `verify` runs public-readiness checks in release context.
5. `verify` runs the ship gate (`macos-ci` when available, otherwise `ci`).
6. `verify` builds release artifacts and release notes.
7. `verify` records manifests for the exact metadata, notes, and artifact files it prepared.
8. `execute` validates that the recorded snapshot still matches `HEAD` and the local files.
9. `execute` creates the release commit and annotated tag locally.
10. `execute` pushes `main` and `v<version>`.
11. `execute` creates or refreshes a GitHub draft release and uploads artifacts while it is still private.
12. `execute` publishes the crate to crates.io.
13. `execute` publishes the GitHub release draft.

That ordering is intentional: crates.io publication no longer happens before the rest of the release is locally finalized, and Ralph does not normalize "crate published, everything else later" as an acceptable steady state.

## Preflight

Run the dependency advisory audit before entering the release transaction:

```bash
make security-audit
```

`make security-audit` requires `cargo-audit` (`cargo install cargo-audit --locked`) and fails on RustSec advisory warnings for the committed `Cargo.lock`. Keep it separate from the release transaction so advisory database/tool availability issues are resolved before release metadata is dirtied.

The release gate also runs `make rust-toolchain-drift-check`, which compares the repo-local `rust-toolchain.toml` override with global rustup stable outside the workspace. If global stable has advanced, intentionally adopt it by updating `rust-toolchain.toml` and `crates/ralph/Cargo.toml` `rust-version` together before rerunning release verification.

`make release-verify VERSION=<x.y.z>` is the canonical release preflight because it now prepares the exact local release snapshot that `make release` will publish:

1. `scripts/release.sh verify <x.y.z>`

If `make release-verify` fails, fix the issue and rerun the full preflight. Do not skip phases manually.

After `make release-verify` succeeds, expect release metadata files such as `VERSION`, `Cargo.lock`, `CHANGELOG.md`, versioned app metadata, and the committed JSON schemas under `schemas/` produced by `make generate` (`schemas/config.schema.json`, `schemas/queue.schema.json`, `schemas/machine.schema.json`) to remain dirty in the working tree until `make release VERSION=<x.y.z>` commits them as the release commit.

## Reconcile

If a remote step fails after local preparation, reconcile the same version explicitly:

```bash
scripts/release.sh reconcile 0.2.0
```

The script reconciles from `target/release-transactions/v0.2.0/state.env` and continues at the next incomplete remote step. Verification snapshots remain under `target/release-verifications/v0.2.0/` as evidence of the prepared publish state.

If reconcile is resuming before crates.io publication, the transaction is still in the reversible portion of the flow. If crates.io publication already succeeded, finish the GitHub release publication immediately rather than treating that state as a normal pause point.

## Artifacts

Release artifacts live under `target/release-artifacts/` and are rebuilt from scratch for each run. The artifact builder uses the shared CLI bundling entrypoint (`scripts/ralph-cli-bundle.sh`) for native release binaries so app bundling and release packaging consume the same CLI build contract.

## Version Metadata

The canonical version source is the top-level `VERSION` file. `scripts/versioning.sh sync` updates:

- `VERSION`
- `Cargo.lock`
- `crates/ralph/Cargo.toml`
- `apps/RalphMac/RalphMac.xcodeproj/project.pbxproj`
- `apps/RalphMac/RalphCore/VersionValidator.swift`

Treat any drift in those files as a release blocker.

Rust baseline changes are owned by `rust-toolchain.toml` and the crate `rust-version`; `scripts/versioning.sh sync` only synchronizes release version metadata. Use `make rust-toolchain-check` for routine internal consistency and `make rust-toolchain-drift-check` before release/public readiness when comparing against global stable.

## Related Docs

- [Release runbook](./guides/release-runbook.md)
- [Public readiness guide](./guides/public-readiness.md)
- [Versioning policy](./versioning-policy.md)
