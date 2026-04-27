#!/usr/bin/env bash
#
# Purpose: Run a repo-wide public-readiness audit for the Ralph repository.
# Responsibilities:
# - Validate required public-facing files and forbid tracked runtime/build artifacts.
# - Scan the repo working tree for broken markdown links and high-confidence secret material.
# - Guard documented runtime paths that affect operator recovery.
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
ALLOW_NO_GIT=0

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
  --allow-no-git    Allow source-snapshot safety mode for `--skip-ci --skip-clean` flows
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
            continue
        fi
        if [ -L "$REPO_ROOT/$path" ]; then
            ralph_log_error "Required file must be a regular repo file, not a symlink: $path"
            missing=1
        fi
    done

    [ "$missing" -eq 0 ]
}

git_worktree_available() {
    git -C "$REPO_ROOT" rev-parse --is-inside-work-tree >/dev/null 2>&1
}

require_git_worktree() {
    if git_worktree_available; then
        return 0
    fi

    ralph_log_error "Pre-public checks require a git worktree; source snapshots cannot validate tracked-file or cleanliness invariants"
    return 1
}

report_path_violations() {
    local message="$1"
    shift

    [ "$#" -eq 0 ] && return 0

    ralph_log_error "$message"
    printf '  %s\n' "$@" >&2
    return 1
}

scan_tracked_paths() {
    local tracked_file
    tracked_file=$(release_collect_git_output_z "git ls-files -z" git -C "$REPO_ROOT" ls-files -z) || return 1

    local path
    local handler
    local failed=0
    local handlers=("$@")

    while IFS= read -r -d '' path; do
        [ -z "$path" ] && continue
        if ! release_require_safe_publication_path "Tracked file list" "$path"; then
            failed=1
            break
        fi
        [ -e "$REPO_ROOT/$path" ] || [ -L "$REPO_ROOT/$path" ] || continue
        for handler in "${handlers[@]}"; do
            if ! "$handler" "$path"; then
                failed=1
                break 2
            fi
        done
    done <"$tracked_file"

    rm -f "$tracked_file"
    [ "$failed" -eq 0 ]
}

collect_tracked_runtime_build_path_violations() {
    local path="$1"
    if release_is_disallowed_tracked_runtime_build_path "$path"; then
        tracked_violations+=("$path")
    fi
}

collect_tracked_ralph_allowlist_violations() {
    local path="$1"
    if [ "$path" = ".ralph" ] || [[ "$path" == .ralph/* ]]; then
        if [ "$path" = ".ralph" ] || [ -L "$REPO_ROOT/$path" ] || ! release_is_allowed_tracked_ralph_path "$path"; then
            unexpected+=("$path")
        fi
    fi
}

collect_tracked_local_only_violations() {
    local path="$1"
    if release_is_local_only_path "$path"; then
        violations+=("$path")
    fi
}

check_source_snapshot_artifacts() {
    ralph_log_info "Checking source snapshot for local/runtime artifacts"

    local violations=()
    local rel_path
    for rel_path in "${PUBLIC_SOURCE_SNAPSHOT_DISALLOWED_PATHS[@]}"; do
        if [ -e "$REPO_ROOT/$rel_path" ] || [ -L "$REPO_ROOT/$rel_path" ]; then
            violations+=("$rel_path")
        fi
    done

    if [ -e "$REPO_ROOT/.ralph" ] || [ -L "$REPO_ROOT/.ralph" ]; then
        if [ ! -d "$REPO_ROOT/.ralph" ] || [ -L "$REPO_ROOT/.ralph" ]; then
            violations+=(".ralph")
        else
            while IFS= read -r -d '' rel_path; do
                [ -z "$rel_path" ] && continue
                rel_path="${rel_path#./}"
                release_require_safe_publication_path "Source snapshot" "$rel_path" || return 1
                if [ -L "$REPO_ROOT/$rel_path" ] || ! release_is_allowed_tracked_ralph_path "$rel_path"; then
                    violations+=("$rel_path")
                fi
            done < <(
                cd "$REPO_ROOT" && find .ralph -mindepth 1 -print0
            )
        fi
    fi

    while IFS= read -r -d '' rel_path; do
        [ -z "$rel_path" ] && continue
        rel_path="${rel_path#./}"
        release_require_safe_publication_path "Source snapshot" "$rel_path" || return 1
        if release_is_local_only_path "$rel_path"; then
            violations+=("$rel_path")
        fi
    done < <(
        cd "$REPO_ROOT" && find . \( -type f -o -type l \) -print0
    )

    if [ "${#violations[@]}" -gt 0 ]; then
        report_path_violations \
            "Source snapshot contains local/runtime artifacts that must be excluded before publication checks can pass" \
            "${violations[@]}" || return 1
    fi

    ralph_log_success "No local/runtime artifacts detected in source snapshot mode"
}

check_tracked_runtime_artifacts() {
    ralph_log_info "Checking tracked runtime/build artifacts"

    local tracked_violations=()
    local unexpected=()

    scan_tracked_paths \
        collect_tracked_runtime_build_path_violations \
        collect_tracked_ralph_allowlist_violations || return 1

    if [ "${#tracked_violations[@]}" -gt 0 ]; then
        report_path_violations "Tracked runtime/build artifacts detected" "${tracked_violations[@]}" || return 1
    fi
    if [ "${#unexpected[@]}" -gt 0 ]; then
        report_path_violations "Tracked .ralph files outside the public allowlist detected" "${unexpected[@]}" || return 1
    fi

    ralph_log_success "No tracked runtime/build artifacts detected"
}

check_local_only_tracking() {
    ralph_log_info "Checking tracked local-only files"

    local violations=()

    scan_tracked_paths collect_tracked_local_only_violations || return 1
    if [ "${#violations[@]}" -gt 0 ]; then
        report_path_violations "Tracked local-only files detected" "${violations[@]}" || return 1
    fi

    ralph_log_success "No tracked local-only files detected"
}

check_worktree_clean() {
    if [ "$SKIP_CLEAN" -eq 1 ]; then
        ralph_log_warn "Skipping clean-worktree check"
        return 0
    fi

    ralph_log_info "Checking git worktree cleanliness"
    local collected_dirty
    collected_dirty=$(release_collect_dirty_lines "$REPO_ROOT") || return 1
    local dirty
    dirty=$(release_filter_dirty_lines "$collected_dirty")
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

check_public_readiness_content() {
    if [ "$SKIP_SECRETS" -eq 1 ] && [ "$SKIP_LINKS" -eq 1 ]; then
        ralph_log_warn "Skipping public-readiness content scans"
        return 0
    fi

    if [ "$SKIP_SECRETS" -eq 1 ]; then
        ralph_log_warn "Skipping secret-pattern scan"
        bash "$SCRIPT_DIR/lib/public_readiness_scan.sh" docs
        return
    fi

    if [ "$SKIP_LINKS" -eq 1 ]; then
        ralph_log_warn "Skipping markdown link checks"
        bash "$SCRIPT_DIR/lib/public_readiness_scan.sh" secrets
        return
    fi

    bash "$SCRIPT_DIR/lib/public_readiness_scan.sh" all
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
            --allow-no-git)
                ALLOW_NO_GIT=1
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

    local has_git=0
    if git_worktree_available; then
        has_git=1
    elif [ "$ALLOW_NO_GIT" -eq 1 ]; then
        if [ "$SKIP_CI" -ne 1 ] || [ "$SKIP_CLEAN" -ne 1 ]; then
            ralph_log_error "--allow-no-git requires --skip-ci and --skip-clean because git-backed release and cleanliness checks remain mandatory otherwise"
            exit 2
        fi
        ralph_log_warn "Git worktree unavailable; skipping tracked-file and cleanliness checks in source-snapshot safety mode"
    else
        require_git_worktree
    fi

    check_required_files
    if [ "$has_git" -eq 1 ]; then
        check_tracked_runtime_artifacts
        check_local_only_tracking
        check_worktree_clean
    else
        check_source_snapshot_artifacts
    fi
    check_public_readiness_content
    run_ci_gate
    if [ "$has_git" -eq 1 ]; then
        check_worktree_clean
    fi

    echo ""
    ralph_log_success "Pre-public checks passed"
}

main "$@"
