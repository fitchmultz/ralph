#!/usr/bin/env bash
#
# Purpose: Run a repo-wide public-readiness audit for the Ralph repository.
# Responsibilities:
# - Validate required public-facing files and forbid tracked runtime/build artifacts.
# - Scan the repo working tree for broken markdown links and high-confidence secret material.
# - Run the local CI gate when requested and enforce clean or release-context worktrees.
# Scope:
# - Repository hygiene and publication safety only; it does not tag or publish releases.
# Usage:
# - scripts/pre-public-check.sh
# - scripts/pre-public-check.sh --skip-ci --release-context
# - scripts/pre-public-check.sh --skip-links --skip-secrets
# Invariants/assumptions:
# - Run from any location; the script resolves repo root automatically.
# - `--release-context` permits only canonical release metadata drift.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/lib/ralph-shell.sh"
REPO_ROOT="$(ralph_repo_root)"
source "$SCRIPT_DIR/lib/release_policy.sh"

SKIP_CI=0
SKIP_LINKS=0
SKIP_SECRETS=0
SKIP_CLEAN=0
RELEASE_CONTEXT=0

usage() {
    cat <<'EOF'
Pre-publication audit for Ralph.

Usage:
  scripts/pre-public-check.sh [OPTIONS]

Options:
  --skip-ci         Skip the shared release gate (`make release-gate`)
  --skip-links      Skip repo-wide working-tree markdown link checks
  --skip-secrets    Skip repo-wide working-tree secret-pattern scan
  --skip-clean      Skip worktree cleanliness checks
  --release-context Allow only canonical release metadata files to be dirty
  -h, --help        Show this help message

Exit codes:
  0  Success
  1  One or more checks failed
  2  Invalid usage
EOF
}

check_required_files() {
    ralph_log_info "Checking required public-facing files"

    local path
    local missing=0
    for path in "${PUBLIC_REQUIRED_FILES[@]}"; do
        if [ ! -f "$REPO_ROOT/$path" ]; then
            ralph_log_error "Missing required file: $path"
            missing=1
        fi
    done

    [ "$missing" -eq 0 ]
}

check_tracked_runtime_artifacts() {
    ralph_log_info "Checking tracked runtime/build artifacts"

    local tracked
    tracked=$(git -C "$REPO_ROOT" ls-files \
        apps/RalphMac/build \
        '.ralph/cache' \
        '.ralph/lock' \
        '.ralph/logs' \
        '.ralph/workspaces' \
        '.ralph/undo' \
        '.ralph/webhooks' || true)

    if [ -n "$tracked" ]; then
        ralph_log_error "Tracked runtime/build artifacts detected"
        printf '  %s\n' "$tracked" >&2
        return 1
    fi

    local tracked_ralph
    tracked_ralph=$(git -C "$REPO_ROOT" ls-files -- '.ralph' || true)
    if [ -n "$tracked_ralph" ]; then
        local unexpected=()
        local path
        while IFS= read -r path; do
            [ -z "$path" ] && continue
            if ! release_is_allowed_tracked_ralph_path "$path"; then
                unexpected+=("$path")
            fi
        done <<< "$tracked_ralph"

        if [ "${#unexpected[@]}" -ne 0 ]; then
            ralph_log_error "Tracked .ralph files outside the public allowlist detected"
            printf '  %s\n' "${unexpected[@]}" >&2
            return 1
        fi
    fi

    ralph_log_success "No tracked runtime/build artifacts detected"
}

check_env_tracking() {
    ralph_log_info "Checking .env tracking"
    local tracked_env
    tracked_env=$(git -C "$REPO_ROOT" ls-files | grep -E '(^|/)\.env($|\.)' | grep -Ev '(^|/)\.env\.example$' || true)
    if [ -n "$tracked_env" ]; then
        ralph_log_error "Tracked env files detected"
        printf '  %s\n' "$tracked_env" >&2
        return 1
    fi
    ralph_log_success "No tracked env files detected"
}

check_worktree_clean() {
    if [ "$SKIP_CLEAN" -eq 1 ]; then
        ralph_log_warn "Skipping clean-worktree check"
        return 0
    fi

    ralph_log_info "Checking git worktree cleanliness"
    local dirty
    dirty=$(git -C "$REPO_ROOT" status --porcelain | grep -vE '^..[[:space:]]+\.ralph/' || true)
    if [ -z "$dirty" ]; then
        ralph_log_success "Working tree is clean"
        return 0
    fi

    if [ "$RELEASE_CONTEXT" -eq 1 ] && release_assert_dirty_paths_allowed "$dirty"; then
        ralph_log_success "Working tree contains release-only metadata drift"
        return 0
    fi

    ralph_log_error "Working tree is not clean"
    echo "$dirty" | sed 's/^/  /' >&2
    return 1
}

check_secret_patterns() {
    if [ "$SKIP_SECRETS" -eq 1 ]; then
        ralph_log_warn "Skipping secret-pattern scan"
        return 0
    fi

    bash "$SCRIPT_DIR/lib/public_readiness_scan.sh" secrets "$REPO_ROOT"
}

check_markdown_links() {
    if [ "$SKIP_LINKS" -eq 1 ]; then
        ralph_log_warn "Skipping markdown link checks"
        return 0
    fi

    bash "$SCRIPT_DIR/lib/public_readiness_scan.sh" links "$REPO_ROOT"
}

run_ci_gate() {
    if [ "$SKIP_CI" -eq 1 ]; then
        ralph_log_warn "Skipping CI gate"
        return 0
    fi

    local make_cmd
    make_cmd=$(ralph_resolve_make_cmd)
    ralph_log_info "Running shared release gate via ${make_cmd} release-gate"
    "$make_cmd" -C "$REPO_ROOT" release-gate
    ralph_log_success "Shared release gate passed"
}

main() {
    while [ $# -gt 0 ]; do
        case "$1" in
            --skip-ci)
                SKIP_CI=1
                ;;
            --skip-links)
                SKIP_LINKS=1
                ;;
            --skip-secrets)
                SKIP_SECRETS=1
                ;;
            --skip-clean)
                SKIP_CLEAN=1
                ;;
            --release-context)
                RELEASE_CONTEXT=1
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

    echo ""
    echo "Pre-public readiness checks"
    echo "=========================="

    check_required_files
    check_tracked_runtime_artifacts
    check_env_tracking
    check_worktree_clean
    check_secret_patterns
    check_markdown_links
    run_ci_gate
    check_worktree_clean

    echo ""
    ralph_log_success "Pre-public checks passed"
}

main "$@"
