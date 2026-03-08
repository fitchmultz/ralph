#!/usr/bin/env bash
#
# Purpose: Classify the current change set into the correct CI surface for agents.
# Responsibilities:
# - Inspect tracked and untracked repo changes.
# - Route docs-only changes to `ci-fast`.
# - Escalate CLI/build/runtime/app contract changes to `macos-ci`.
# Scope:
# - Classification only; it does not execute make targets itself.
# Usage:
# - scripts/agent-ci-surface.sh --target
# - scripts/agent-ci-surface.sh --reason
# Invariants/assumptions:
# - When no git worktree is available, callers should conservatively run `macos-ci`.

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

Outputs:
  --target   Print the target name (`ci-fast` or `macos-ci`)
  --reason   Print a short routing explanation
EOF
}

while [ $# -gt 0 ]; do
    case "$1" in
        --target)
            MODE="target"
            ;;
        --reason)
            MODE="reason"
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
    if [ "$MODE" = "reason" ]; then
        echo "not in a git worktree"
    else
        echo "macos-ci"
    fi
    exit 0
fi

changed_paths="$(
    {
        git -C "$REPO_ROOT" diff --name-only --relative
        git -C "$REPO_ROOT" diff --cached --name-only --relative
        git -C "$REPO_ROOT" ls-files --others --exclude-standard
    } | sed '/^$/d' | sort -u
)"

if [ -z "$changed_paths" ]; then
    if [ "$MODE" = "reason" ]; then
        echo "no pending changes; defaulting to ci-fast"
    else
        echo "ci-fast"
    fi
    exit 0
fi

target="ci-fast"
reason="docs/community metadata only"
while IFS= read -r path; do
    [ -z "$path" ] && continue
    if public_requires_macos_ci_for_path "$path"; then
        target="macos-ci"
        reason="dependency-surface change touched app/CLI/build/runtime contract: $path"
        break
    fi
    if ! public_is_docs_only_path "$path"; then
        target="macos-ci"
        reason="non-doc change requires full app/CLI verification: $path"
        break
    fi
done <<< "$changed_paths"

if [ "$MODE" = "reason" ]; then
    echo "$reason"
else
    echo "$target"
fi
