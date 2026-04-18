#!/usr/bin/env bash
#
# Purpose: Classify the current change set into the correct CI surface for agents.
# Responsibilities:
# - Inspect the current local working-tree delta (unstaged, staged, and untracked paths).
# - Route docs/community-only surfaces to `ci-docs`.
# - Route ancillary non-code surfaces to `ci-fast`.
# - Route Rust crate work to `ci` (release-shaped Rust gate).
# - Escalate app/toolchain/bundling/schema/script surfaces to `macos-ci`.
# Scope:
# - Classification only; it does not execute make targets itself.
# Usage:
# - scripts/agent-ci-surface.sh --target
# - scripts/agent-ci-surface.sh --reason
# - scripts/agent-ci-surface.sh --emit-eval   # shell-evaluable RALPH_AGENT_CI_* assignments
# Invariants/assumptions:
# - When no git worktree is available, callers should conservatively run `macos-ci`.
# - `ci-docs` is reserved for changes that cannot alter executable behavior.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/lib/ralph-shell.sh"
REPO_ROOT="$(ralph_repo_root)"
source "$SCRIPT_DIR/lib/release_policy.sh"

MODE="target"

usage() {
    cat <<'EOF'
Usage:
  scripts/agent-ci-surface.sh --target
  scripts/agent-ci-surface.sh --reason
  scripts/agent-ci-surface.sh --emit-eval

Outputs:
  --target     Print the target name (`noop`, `ci-docs`, `ci-fast`, `ci`, or `macos-ci`)
  --reason     Print a short routing explanation
  --emit-eval  Print `RALPH_AGENT_CI_TARGET=...` and `RALPH_AGENT_CI_REASON=...` for `eval` in bash
EOF
}

emit_result() {
    local target="$1"
    local reason="$2"
    case "$MODE" in
        target) printf '%s\n' "$target" ;;
        reason) printf '%s\n' "$reason" ;;
        emit-eval)
            printf 'RALPH_AGENT_CI_TARGET=%q\n' "$target"
            printf 'RALPH_AGENT_CI_REASON=%q\n' "$reason"
            ;;
    esac
}

target_rank() {
    case "$1" in
        noop) printf '0\n' ;;
        ci-docs) printf '1\n' ;;
        ci-fast) printf '2\n' ;;
        ci) printf '3\n' ;;
        macos-ci) printf '4\n' ;;
        *) printf '0\n' ;;
    esac
}

consider_candidate() {
    local candidate_target="$1"
    local candidate_reason="$2"
    if [ "$(target_rank "$candidate_target")" -gt "$(target_rank "$target")" ]; then
        target="$candidate_target"
        reason="$candidate_reason"
    fi
}

while [ $# -gt 0 ]; do
    case "$1" in
        --target)
            MODE="target"
            ;;
        --reason)
            MODE="reason"
            ;;
        --emit-eval)
            MODE="emit-eval"
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        *)
            ralph_log_error "Unknown option: $1"
            usage
            exit 2
            ;;
    esac
    shift
done

if ! git -C "$REPO_ROOT" rev-parse --is-inside-work-tree >/dev/null 2>&1; then
    emit_result "macos-ci" "not in a git worktree"
    exit 0
fi

changed_paths="$(
    {
        git -C "$REPO_ROOT" diff --name-only --relative
        git -C "$REPO_ROOT" diff --cached --name-only --relative
        git -C "$REPO_ROOT" ls-files --others --exclude-standard
    } | sed '/^$/d' | sort -u
)"

combined_local_diff_for_path() {
    local path="$1"
    {
        git -C "$REPO_ROOT" diff --unified=0 --relative -- "$path"
        git -C "$REPO_ROOT" diff --cached --unified=0 --relative -- "$path"
    } 2>/dev/null
}

makefile_diff_requires_macos_ship_gate() {
    local diff
    diff="$(combined_local_diff_for_path "Makefile")"
    case "$diff" in
        *"macos-preflight:"*|*"macos-build:"*|*"macos-install-app:"*|*"macos-test:"*|*"macos-test-contracts:"*|*"macos-test-settings-smoke:"*|*"macos-test-workspace-routing-contract:"*|*"XCODE_"*|*"RALPH_XCODE_"*|*"MACOS_APP_INSTALL_DIR"*|*"scripts/ralph-cli-bundle.sh"*|*"xcodebuild"*)
            return 0
            ;;
    esac
    return 1
}

makefile_diff_requires_rust_release_gate() {
    local diff
    diff="$(combined_local_diff_for_path "Makefile")"
    case "$diff" in
        *"build:"*|*"generate:"*|*"install:"*|*"RALPH_RELEASE_BUILD_STAMP"*|*"RALPH_STAMP_DIR"*|*"RALPH_CLI_BUILD_JOBS_ARG"*|*"BIN_NAME"*|*"BIN_DIR"*|*"PREFIX"*|*"release-gate:"*|*"profile-ship-gate:"*|*"scripts/ralph-cli-bundle.sh"*)
            return 0
            ;;
    esac
    return 1
}

classify_special_path() {
    local path="$1"
    CLASSIFY_TARGET=""
    CLASSIFY_REASON=""

    if [ "$path" = "Makefile" ]; then
        if makefile_diff_requires_macos_ship_gate; then
            CLASSIFY_TARGET="macos-ci"
            CLASSIFY_REASON="Makefile app/macOS build change requires macOS ship gate: $path"
            return 0
        fi
        if makefile_diff_requires_rust_release_gate; then
            CLASSIFY_TARGET="ci"
            CLASSIFY_REASON="Makefile release/build change requires release-shaped verification: $path"
            return 0
        fi
        CLASSIFY_TARGET="ci-fast"
        CLASSIFY_REASON="Makefile CI/router change requires fast Rust/CLI verification: $path"
        return 0
    fi

    case "$path" in
        scripts/*)
            if public_requires_macos_ship_gate_for_script_path "$path"; then
                CLASSIFY_TARGET="macos-ci"
                CLASSIFY_REASON="script change requires macOS ship gate (bundling/Xcode/macOS contract): $path"
                return 0
            fi
            if public_requires_rust_release_gate_for_script_path "$path"; then
                CLASSIFY_TARGET="ci"
                CLASSIFY_REASON="release/build script change requires release-shaped verification: $path"
                return 0
            fi
            CLASSIFY_TARGET="ci-fast"
            CLASSIFY_REASON="CI/router/tooling script change requires fast Rust/CLI verification: $path"
            return 0
            ;;
    esac

    return 1
}

if [ -z "$changed_paths" ]; then
    emit_result "noop" "no local changes; nothing to validate"
    exit 0
fi

all_docs_only=1
while IFS= read -r path; do
    [ -z "$path" ] && continue
    if ! public_is_docs_only_path "$path"; then
        all_docs_only=0
        break
    fi
done <<< "$changed_paths"

if [ "$all_docs_only" = "1" ]; then
    emit_result "ci-docs" "docs/community metadata only"
    exit 0
fi

target="noop"
reason="no local changes; nothing to validate"

while IFS= read -r path; do
    [ -z "$path" ] && continue
    if classify_special_path "$path"; then
        consider_candidate "$CLASSIFY_TARGET" "$CLASSIFY_REASON"
        continue
    fi
    if public_requires_macos_ship_gate_for_path "$path"; then
        consider_candidate "macos-ci" "dependency-surface change requires macOS ship gate (app/toolchain/bundle/schemas): $path"
        continue
    fi
    if public_requires_rust_release_gate_for_path "$path"; then
        consider_candidate "ci" "Rust crate change requires release-shaped verification: $path"
        continue
    fi
    consider_candidate "ci-fast" "non-docs change requires fast Rust/CLI verification: $path"
done <<< "$changed_paths"

emit_result "$target" "$reason"
