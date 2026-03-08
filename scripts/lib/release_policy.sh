#!/usr/bin/env bash
#
# Purpose: Centralize release, public-readiness, and CI-surface policy for Ralph.
# Responsibilities:
# - Define the canonical release metadata file set.
# - Define repo-wide publication checks and CI surface classification rules.
# - Provide reusable helpers for scripts and tests.
# Scope:
# - Policy only; command orchestration lives in the calling scripts.
# Usage:
# - source "$(dirname "$0")/lib/release_policy.sh"
# Invariants/assumptions:
# - Caller has already defined REPO_ROOT and sourced ralph-shell helpers.

if [ -n "${RALPH_RELEASE_POLICY_SOURCED:-}" ]; then
    return 0
fi
RALPH_RELEASE_POLICY_SOURCED=1

RELEASE_METADATA_PATHS=(
    "VERSION"
    "Cargo.lock"
    "crates/ralph/Cargo.toml"
    "apps/RalphMac/RalphMac.xcodeproj/project.pbxproj"
    "apps/RalphMac/RalphCore/VersionValidator.swift"
    "CHANGELOG.md"
    "schemas/config.schema.json"
    "schemas/queue.schema.json"
)

RALPH_TRACKED_ALLOWLIST=(
    ".ralph/README.md"
    ".ralph/queue.jsonc"
    ".ralph/queue.json"
    ".ralph/done.jsonc"
    ".ralph/done.json"
    ".ralph/config.jsonc"
    ".ralph/config.json"
)

PUBLIC_REQUIRED_FILES=(
    "README.md"
    "LICENSE"
    "CHANGELOG.md"
    "CONTRIBUTING.md"
    "SECURITY.md"
    "CODE_OF_CONDUCT.md"
    "docs/guides/public-readiness.md"
    "docs/guides/release-runbook.md"
    "docs/releasing.md"
    ".github/ISSUE_TEMPLATE/bug_report.md"
    ".github/ISSUE_TEMPLATE/feature_request.md"
    ".github/PULL_REQUEST_TEMPLATE.md"
)

MARKDOWN_SCAN_EXCLUDES=(
    ".git/"
    "target/"
    "apps/RalphMac/build/"
)

RELEASE_TRANSACTION_DIR="$REPO_ROOT/target/release-transactions"
RELEASE_VERIFY_DIR_ROOT="$REPO_ROOT/target/release-verifications"
RELEASE_ARTIFACTS_DIR="$REPO_ROOT/target/release-artifacts"
RELEASE_NOTES_DIR="$REPO_ROOT/target/release-notes"

release_is_metadata_path() {
    local path="$1"
    local allowed
    for allowed in "${RELEASE_METADATA_PATHS[@]}"; do
        if [ "$path" = "$allowed" ]; then
            return 0
        fi
    done
    return 1
}

release_assert_dirty_paths_allowed() {
    local dirty_lines="$1"
    local line
    local path
    local disallowed=()

    if [ -z "$dirty_lines" ]; then
        return 0
    fi

    while IFS= read -r line; do
        [ -z "$line" ] && continue
        path=$(echo "$line" | awk '{print $NF}')
        if ! release_is_metadata_path "$path"; then
            disallowed+=("$line")
        fi
    done <<< "$dirty_lines"

    if [ "${#disallowed[@]}" -ne 0 ]; then
        ralph_log_error "Unexpected tracked changes detected"
        printf '  %s\n' "${disallowed[@]}" >&2
        echo "  Allowed release metadata paths are:" >&2
        printf '    - %s\n' "${RELEASE_METADATA_PATHS[@]}" >&2
        return 1
    fi

    return 0
}

release_is_allowed_tracked_ralph_path() {
    local path="$1"
    local allowed
    for allowed in "${RALPH_TRACKED_ALLOWLIST[@]}"; do
        if [ "$path" = "$allowed" ]; then
            return 0
        fi
    done
    return 1
}

public_is_docs_only_path() {
    local path="$1"
    case "$path" in
        *.md|docs/*|.github/ISSUE_TEMPLATE/*|.github/PULL_REQUEST_TEMPLATE.md|LICENSE|CODE_OF_CONDUCT.md|SECURITY.md|CONTRIBUTING.md)
            return 0
            ;;
    esac
    return 1
}

public_requires_macos_ci_for_path() {
    local path="$1"
    case "$path" in
        apps/RalphMac/*|apps/AGENTS.md|crates/*|schemas/*|scripts/*|VERSION|Cargo.toml|Cargo.lock|Makefile|rust-toolchain.toml|.cargo/*)
            return 0
            ;;
    esac
    return 1
}
