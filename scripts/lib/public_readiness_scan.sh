#!/usr/bin/env bash
#
# Purpose: Run focused public-readiness scans for the Ralph repository.
# Responsibilities:
# - Reuse the shared repo-wide markdown-link and secret-pattern scan policy.
# - Provide lightweight entrypoints for docs-only and targeted safety gates.
# - Resolve the Ralph repository root and exclusion policy consistently.
# Scope:
# - Focused scan execution only; required-file checks, worktree checks, and CI gating stay in pre-public-check.sh.
# Usage:
# - scripts/lib/public_readiness_scan.sh links
# - scripts/lib/public_readiness_scan.sh secrets
# - scripts/lib/public_readiness_scan.sh --help
# Invariants/assumptions:
# - Run from any location; the script resolves the repo root automatically.
# - Scan excludes come from scripts/lib/release_policy.sh.

set -euo pipefail

LIB_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SCRIPT_DIR="$(cd "$LIB_DIR/.." && pwd)"
source "$SCRIPT_DIR/lib/ralph-shell.sh"
REPO_ROOT="$(ralph_repo_root)"
source "$SCRIPT_DIR/lib/release_policy.sh"

usage() {
    cat <<'EOF'
Run a focused public-readiness scan for Ralph.

Usage:
  scripts/lib/public_readiness_scan.sh <links|secrets>
  scripts/lib/public_readiness_scan.sh -h
  scripts/lib/public_readiness_scan.sh --help

Examples:
  scripts/lib/public_readiness_scan.sh links
  scripts/lib/public_readiness_scan.sh secrets

Exit codes:
  0  Scan passed
  1  Scan failed
  2  Invalid usage
EOF
}

run_scan() {
    local mode="$1"
    local scan_py_path="${RALPH_PUBLIC_READINESS_SCAN_PY:-$SCRIPT_DIR/lib/public_readiness_scan.py}"
    local repo_root="$REPO_ROOT"

    export RALPH_PUBLIC_SCAN_EXCLUDES
    RALPH_PUBLIC_SCAN_EXCLUDES="$(printf '%s\n' "${PUBLIC_SCAN_EXCLUDES[@]}")"

    case "$mode" in
        links)
            ralph_log_info "Checking repo-wide working-tree markdown links"
            python3 "$scan_py_path" links "$repo_root"
            ralph_log_success "Markdown links look valid"
            ;;
        secrets)
            ralph_log_info "Scanning repo-wide working-tree text files for high-confidence secret patterns"
            python3 "$scan_py_path" secrets "$repo_root"
            ralph_log_success "No high-confidence secret patterns found"
            ;;
        *)
            usage >&2
            exit 2
            ;;
    esac
}

case "${1:-}" in
    links|secrets)
        if [ "$#" -ne 1 ]; then
            usage >&2
            exit 2
        fi
        run_scan "$1"
        ;;
    -h|--help)
        if [ "$#" -ne 1 ]; then
            usage >&2
            exit 2
        fi
        usage
        ;;
    *)
        usage >&2
        exit 2
        ;;
esac
