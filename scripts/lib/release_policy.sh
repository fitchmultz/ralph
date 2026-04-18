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
set -euo pipefail

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
    ".ralph/done.jsonc"
    ".ralph/config.jsonc"
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

PUBLIC_SCAN_EXCLUDES=(
    ".git/"
    "target/"
    ".ralph/cache/"
    ".ralph/lock/"
    ".ralph/logs/"
    ".ralph/plugins/"
    ".ralph/trust.json"
    ".ralph/trust.jsonc"
    ".ralph/undo/"
    ".ralph/webhooks/"
    ".ralph/workspaces/"
    "apps/RalphMac/build/"
    ".venv/"
    ".ruff_cache/"
    ".pytest_cache/"
    ".ty_cache/"
)

PUBLIC_LOCAL_ONLY_BASENAMES=(
    ".DS_Store"
    ".env"
    ".envrc"
    ".scratchpad.md"
    ".FIX_TRACKING.md"
)

PUBLIC_LOCAL_ONLY_BASENAME_PREFIXES=(
    ".env."
)

PUBLIC_SOURCE_SNAPSHOT_DISALLOWED_PATHS=(
    "target"
    "apps/RalphMac/build"
    ".venv"
    ".ruff_cache"
    ".pytest_cache"
    ".ty_cache"
)

PUBLIC_TRACKED_RUNTIME_BUILD_PREFIXES=(
    "target/"
    "apps/RalphMac/build/"
    ".venv/"
    ".ruff_cache/"
    ".pytest_cache/"
    ".ty_cache/"
    ".ralph/cache/"
    ".ralph/lock/"
    ".ralph/logs/"
    ".ralph/workspaces/"
    ".ralph/undo/"
    ".ralph/webhooks/"
)

PUBLIC_IGNORED_DIRTY_PATHS=(
    ".ralph/trust.json"
    ".ralph/trust.jsonc"
)

PUBLIC_IGNORED_DIRTY_PATH_PREFIXES=(
    ".ralph/cache/"
    ".ralph/lock/"
    ".ralph/logs/"
    ".ralph/plugins/"
    ".ralph/undo/"
    ".ralph/webhooks/"
    ".ralph/workspaces/"
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

release_parse_dirty_line_path() {
    local line="$1"

    if [ "${#line}" -lt 4 ]; then
        return 1
    fi

    printf '%s\n' "${line:3}"
}

release_format_path_for_logs() {
    printf '%q' "$1"
}

release_path_has_control_characters() {
    python3 -c 'import sys; data = sys.argv[1].encode("utf-8", "surrogateescape"); sys.exit(0 if any(byte < 32 or byte == 127 for byte in data) else 1)' "$1"
}

release_require_safe_publication_path() {
    local context="$1"
    local path="$2"
    local control_status=0

    if release_path_has_control_characters "$path"; then
        ralph_log_error "$context contains a path with unsupported control characters: $(release_format_path_for_logs "$path")"
        return 1
    else
        control_status=$?
    fi

    if [ "$control_status" -ne 1 ]; then
        ralph_log_error "$context path validation failed for $(release_format_path_for_logs "$path")"
        return 1
    fi

    return 0
}

release_dirty_status_has_second_path() {
    local status="$1"

    case "$status" in
        [RC]?|?[RC])
            return 0
            ;;
    esac

    return 1
}

release_collect_git_output_z() {
    local context="$1"
    shift

    local output_file
    output_file=$(ralph_mktemp_file "ralph-git-output")
    if ! "$@" >"$output_file" 2>/dev/null; then
        rm -f "$output_file"
        ralph_log_error "$context failed"
        return 1
    fi

    printf '%s\n' "$output_file"
}

release_collect_dirty_lines() {
    local repo_root="${1:-$REPO_ROOT}"
    local entry
    local status
    local path
    local lines=()
    local failed=0
    local dirty_file

    dirty_file=$(release_collect_git_output_z "git status --porcelain=v1 -z" git -C "$repo_root" status --porcelain=v1 -z) || return 1

    while IFS= read -r -d '' entry; do
        [ -z "$entry" ] && continue
        status="${entry:0:2}"
        path="${entry:3}"
        if ! release_require_safe_publication_path "Git status" "$path"; then
            failed=1
            break
        fi
        lines+=("$status $path")

        if release_dirty_status_has_second_path "$status"; then
            if ! IFS= read -r -d '' path; then
                ralph_log_error "Git status rename/copy entry ended unexpectedly"
                failed=1
                break
            fi
            if ! release_require_safe_publication_path "Git status" "$path"; then
                failed=1
                break
            fi
            lines+=("$status $path")
        fi
    done <"$dirty_file"

    rm -f "$dirty_file"
    if [ "$failed" -ne 0 ]; then
        return 1
    fi

    if [ "${#lines[@]}" -ne 0 ]; then
        printf '%s\n' "${lines[@]}"
    fi
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
        path=$(release_parse_dirty_line_path "$line") || {
            disallowed+=("$line")
            continue
        }
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

release_is_local_only_name() {
    local name="$1"
    local exact
    local prefix

    for exact in "${PUBLIC_LOCAL_ONLY_BASENAMES[@]}"; do
        if [ "$name" = "$exact" ]; then
            return 0
        fi
    done

    for prefix in "${PUBLIC_LOCAL_ONLY_BASENAME_PREFIXES[@]}"; do
        if [[ "$name" == "$prefix"* ]] && [ "$name" != ".env.example" ]; then
            return 0
        fi
    done

    return 1
}

release_is_local_only_path() {
    local path="${1#./}"
    local component

    IFS='/' read -r -a components <<< "$path"
    for component in "${components[@]}"; do
        [ -z "$component" ] && continue
        if release_is_local_only_name "$component"; then
            return 0
        fi
    done

    return 1
}

release_path_matches_exact_or_dir_prefix() {
    local path="${1#./}"
    local prefix="${2#./}"
    local exact="${prefix%/}"

    if [ "$path" = "$exact" ]; then
        return 0
    fi

    if [[ "$path" == "$prefix"* ]]; then
        return 0
    fi

    return 1
}

release_is_disallowed_tracked_runtime_build_path() {
    local path="${1#./}"
    local prefix

    for prefix in "${PUBLIC_TRACKED_RUNTIME_BUILD_PREFIXES[@]}"; do
        if release_path_matches_exact_or_dir_prefix "$path" "$prefix"; then
            return 0
        fi
    done

    return 1
}

release_is_ignored_dirty_path() {
    local path="${1#./}"
    local exact
    local prefix

    for exact in "${PUBLIC_IGNORED_DIRTY_PATHS[@]}"; do
        if [ "$path" = "$exact" ]; then
            return 0
        fi
    done

    for prefix in "${PUBLIC_IGNORED_DIRTY_PATH_PREFIXES[@]}"; do
        if [[ "$path" == "$prefix"* ]]; then
            return 0
        fi
    done

    return 1
}

release_filter_dirty_lines() {
    local dirty_lines="$1"
    local filtered=()
    local line
    local path

    while IFS= read -r line; do
        [ -z "$line" ] && continue
        path=$(release_parse_dirty_line_path "$line") || {
            filtered+=("$line")
            continue
        }
        if release_is_ignored_dirty_path "$path"; then
            continue
        fi
        filtered+=("$line")
    done <<< "$dirty_lines"

    if [ "${#filtered[@]}" -ne 0 ]; then
        printf '%s\n' "${filtered[@]}"
    fi
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

# Tier D (`make macos-ci`): app bundle, toolchain, committed schemas, and macOS-specific build surfaces.
public_requires_macos_ship_gate_for_path() {
    local path="$1"
    case "$path" in
        apps/RalphMac/*|apps/AGENTS.md|schemas/*|VERSION|Cargo.toml|Cargo.lock|rust-toolchain.toml|.cargo/*)
            return 0
            ;;
    esac
    return 1
}

# Tier D script subset: bundling, macOS app contracts, and Xcode locking.
public_requires_macos_ship_gate_for_script_path() {
    local path="$1"
    case "$path" in
        scripts/ralph-cli-bundle.sh|scripts/macos-*|scripts/lib/xcodebuild-lock.sh)
            return 0
            ;;
    esac
    return 1
}

# Tier C (`make ci`): Rust crate sources and crate-local metadata (release-shaped CLI gate).
public_requires_rust_release_gate_for_path() {
    local path="$1"
    case "$path" in
        crates/*)
            return 0
            ;;
    esac
    return 1
}

# Tier C script subset: release/build metadata plumbing that does not require Xcode app validation.
public_requires_rust_release_gate_for_script_path() {
    local path="$1"
    case "$path" in
        scripts/build-release-artifacts.sh|scripts/release.sh|scripts/versioning.sh|scripts/profile-ship-gate.sh)
            return 0
            ;;
    esac
    return 1
}

# Backward-compatible union used by older call sites: true if either ship gate or Rust crates changed.
public_requires_macos_ci_for_path() {
    if public_requires_macos_ship_gate_for_path "$1"; then
        return 0
    fi
    if public_requires_macos_ship_gate_for_script_path "$1"; then
        return 0
    fi
    if public_requires_rust_release_gate_for_path "$1"; then
        return 0
    fi
    if public_requires_rust_release_gate_for_script_path "$1"; then
        return 0
    fi
    return 1
}
