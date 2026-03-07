# Versioning Policy

Purpose: define how Ralph versions releases and communicates compatibility changes.

## Scheme

Ralph follows semantic versioning:

- `MAJOR`: breaking CLI/config/behavior changes
- `MINOR`: backward-compatible features
- `PATCH`: backward-compatible fixes

## Compatibility Expectations

- Public command behavior changes must be documented in:
  - `CHANGELOG.md`
  - relevant docs under `docs/`
- Breaking changes require migration notes in release docs
- Config schema changes must keep validation/error messaging explicit

## Deprecation Policy

- Prefer explicit deprecation windows for user-facing commands/options
- Document deprecations in changelog before removal when feasible
- Remove dead/deprecated paths promptly once cutover is complete

## Release Hygiene

Before tagging:

```bash
make ci
make pre-public-check
```

If macOS app changes are included:

```bash
make macos-ci
```

Release/versioning invariants:

- `VERSION` is the canonical source of truth.
- `scripts/versioning.sh sync --version <x.y.z>` is the only supported way to bump release metadata.
- `Cargo.lock` is part of synchronized version metadata and must be committed with release bumps.
- `scripts/release.sh` owns `target/release-artifacts/` and clears stale tarballs before packaging.
- Prefer the pinned toolchain from [`rust-toolchain.toml`](../rust-toolchain.toml) when running release gates; if your shell resolves an older `rustc`, use the rustup toolchain bin dir explicitly.
