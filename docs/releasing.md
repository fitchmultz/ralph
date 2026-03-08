# Releasing Ralph

Ralph releases now use an explicit transaction model. The release flow prepares every local mutation first, records transaction state under `target/release-transactions/`, and only then performs irreversible remote publication.

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
| `scripts/release.sh verify <version>` | Validate the release contract without mutating repo or remote state |
| `scripts/release.sh execute <version>` | Prepare local release state, then publish through the transaction pipeline |
| `scripts/release.sh reconcile <version>` | Resume a previously recorded transaction for the same version |

The mutating flow runs in this order:

1. Check prerequisites and repo state.
2. Sync version metadata from `VERSION`.
3. Generate/promote changelog entries.
4. Run public-readiness checks in release context.
5. Run the ship gate (`macos-ci` when available, otherwise `ci`).
6. Build release artifacts and release notes.
7. Create the release commit and annotated tag locally.
8. Publish the crate to crates.io.
9. Push `main` and `v<version>`.
10. Create the GitHub release and upload artifacts.

That ordering is intentional: crates.io publication no longer happens before the rest of the release is locally finalized.

## Preflight

`make release-verify VERSION=<x.y.z>` is the canonical preflight because it exercises the same policy surfaces as the real release:

1. `./scripts/versioning.sh sync --version <x.y.z>`
2. `./scripts/versioning.sh check`
3. `scripts/pre-public-check.sh --skip-ci --release-context`
4. `make release-gate`
5. `scripts/release.sh verify <x.y.z>`

If `make release-verify` fails, fix the issue and rerun the full preflight. Do not skip phases manually.

## Reconcile

If a remote step fails after local preparation, reconcile the same version explicitly:

```bash
scripts/release.sh reconcile 0.2.0
```

The script reconciles from `target/release-transactions/v0.2.0/state.env` and continues at the next incomplete remote step. This replaces the older ad hoc skip-publish workflow.

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

## Related Docs

- [Release runbook](./guides/release-runbook.md)
- [Public readiness guide](./guides/public-readiness.md)
- [Versioning policy](./versioning-policy.md)
