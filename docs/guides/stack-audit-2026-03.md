# Stack Audit (2026-03)
Status: Archived
Owner: Maintainers
Source of truth: historical snapshot; current guidance lives in linked active docs
Parent: [Ralph Documentation](../index.md)


Purpose: record the current language/toolchain/dependency baseline, note the official best-practice guidance reviewed during the March 2026 audit, and document the cutover actions taken.

## Scope

- Rust CLI workspace under `crates/ralph/`
- macOS SwiftUI app under `apps/RalphMac/`
- Local build/test entrypoints in `Makefile`

## Current Versions

Audit date: `2026-03-06`

### Languages and Toolchains

- Rust toolchain: `1.94.0` stable (`rust-toolchain.toml`)
- Cargo manifest MSRV floor: `1.94` (`crates/ralph/Cargo.toml`)
- Rust edition: `2024`
- Xcode: `26.3`
- Swift language mode: `6.2`
- macOS deployment target: `15.0`
- GNU Make: `>= 4`

### Rust Direct Dependencies

- `anyhow 1.0.102`
- `clap 4.5.60`
- `clap_complete 4.5`
- `csv 1.4`
- `colored 3`
- `ctrlc 3.5.2`
- `dotenvy 0.15.7`
- `env_logger 0.11.9`
- `log 0.4.29`
- `serde 1.0.228`
- `serde_json 1.0`
- `serde_yaml 0.9`
- `jsonc-parser 0.29`
- `tempfile 3.26.0`
- `regex 1.12`
- `schemars 1.2`
- `thiserror 2.0.18`
- `time 0.3.47`
- `dialoguer 0.12`
- `atty 0.2`
- `notify-rust 4` (optional)
- `chrono 0.4`
- `indicatif 0.18`
- `notify 8`
- `nucleo-matcher 0.3`
- `globset 0.4`
- `ureq 3`
- `hmac 0.12`
- `sha2 0.10`
- `hex 0.4`
- `crossbeam-channel 0.5`
- Unix-only: `libc 0.2`, `signal-hook 0.4`
- Windows-only: `windows-sys 0.61`
- Dev: `serial_test 3.4.0`, `insta 1.46.3`
- Build: `vergen-gitcl 9.1`

### Swift Dependency Audit

- No Swift Package Manager, CocoaPods, Carthage, or Tuist package manifests are present.
- The macOS app is currently a first-party Xcode project without third-party Swift package dependencies to upgrade.

## Best-Practice Review

### Rust 1.94 / Edition 2024

Reviewed:

- Rust `1.94.0` release notes
- Edition `2024` baseline already configured in `crates/ralph/Cargo.toml`

Current alignment:

- The repo now pins the current stable Rust toolchain instead of trailing one stable release.
- The workspace already uses Edition 2024, so no edition migration work was needed during the toolchain bump.
- Local CI remains driven through `make` targets, which keeps formatting, linting, tests, docs, and app verification on the same pinned toolchain.

Action taken:

- Updated `rust-toolchain.toml` from `1.93.1` to `1.94.0`.
- Updated `rust-version` from `1.93` to `1.94`.

### clap 4

Reviewed:

- Official clap derive-oriented parser documentation

Current alignment:

- CLI parsing already uses derive-based `Parser` / `Args` / `Subcommand` / `ValueEnum`, which is the recommended clap 4 style.
- No migration from builder APIs or deprecated v3 patterns was needed.

### env_logger 0.11

Reviewed:

- Official `env_logger::Builder` documentation

Current alignment:

- Startup already configures logging via `env_logger::Builder::from_default_env()`, which keeps log control environment-driven and matches the current crate guidance.

### signal-hook 0.4

Reviewed:

- Official `signal_hook::iterator::Signals` documentation

Current alignment:

- Daemon signal handling already uses `Signals`, which is the current supported iterator-based pattern.

### ureq 3

Reviewed:

- Official `ureq` agent configuration documentation

Current alignment:

- Webhook delivery already uses `ureq::Agent::config_builder()` instead of ad hoc per-request setup, matching the crate's current configuration model.

### Swift 6.2 / Xcode 26.3

Reviewed:

- Swift `6.2` announcement and concurrency guidance

Current alignment:

- The app already builds in Swift `6.2` mode on Xcode `26.3`.
- Project settings already enforce `SWIFT_STRICT_CONCURRENCY = complete`.
- The codebase already leans into Swift 6 isolation guidance with pervasive `@MainActor` annotations in UI-facing types and `Sendable` modeling in shared/data types.
- There are no external Swift packages to migrate.

## Verification

Commands used during the audit/cutover:

```bash
cargo outdated --root-deps-only --depth 1
make update
make agent-ci
make macos-ci
```

Expected outcome:

- Rust direct dependencies are up to date.
- Rust CLI gates pass on pinned stable Rust `1.94.0`.
- macOS app builds and tests pass on Xcode `26.3` / Swift `6.2`.

## Sources

- Rust `1.94.0` release notes: <https://blog.rust-lang.org/2026/03/05/Rust-1.94.0/>
- Swift `6.2` announcement: <https://www.swift.org/blog/announcing-swift-6.2/>
- clap derive docs: <https://docs.rs/clap/latest/clap/_derive/>
- env_logger builder docs: <https://docs.rs/env_logger/latest/env_logger/struct.Builder.html>
- signal-hook iterator docs: <https://docs.rs/signal-hook/latest/signal_hook/iterator/struct.Signals.html>
- ureq configuration docs: <https://docs.rs/ureq/latest/ureq/struct.Agent.html>
