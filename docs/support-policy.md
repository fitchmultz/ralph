# Support Policy
Status: Active
Owner: Maintainers
Source of truth: this document for its stated scope
Parent: [Ralph Documentation](index.md)


Purpose: clarify supported platforms/tooling and what maintainers can realistically support.

## Supported Platforms

- Linux: supported for CLI workflows
- macOS: supported for CLI workflows and the SwiftUI app
- Windows: best-effort for CLI where dependency chain permits; no first-class app support

## Tooling Baseline

- Rust toolchain pinned by `rust-toolchain.toml`
- GNU Make >= 4 required for project targets
- Optional tools:
  - `cargo-nextest` (faster non-doc test runs)
  - `cargo-llvm-cov` (coverage)
  - Xcode (macOS app build/test)

## Current Audited Baseline

As of 2026-03-06:

- Rust `1.94.0` (stable), pinned via `rust-toolchain.toml`
- Xcode `26.3`
- Swift language mode `6.2`
- macOS deployment target `15.0`

Best-practice checks live in `docs/guides/stack-audit-2026-03.md`.

## Support Windows

- Current release line: actively supported
- Older releases: best-effort only unless explicitly called out in release notes

## Issue Triage Expectations

When filing issues, include:

- exact command + output
- OS + toolchain versions
- whether failure reproduces on clean clone

Use:

- bug reports: `.github/ISSUE_TEMPLATE/bug_report.md`
- feature requests: `.github/ISSUE_TEMPLATE/feature_request.md`
