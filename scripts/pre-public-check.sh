#!/usr/bin/env bash
#
# Purpose: Run a repo-wide public-readiness audit for the Ralph repository.
# Responsibilities:
# - Validate required public-facing files and forbid tracked runtime/build artifacts.
# - Scan the whole repo for broken markdown links and obvious secret material.
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
  --skip-links      Skip repo-wide markdown link checks
  --skip-secrets    Skip repo-wide secret-pattern scan
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

    ralph_log_info "Scanning repo-wide files for obvious secret patterns"

    python3 - "$REPO_ROOT" <<'PY'
from pathlib import Path
import re
import subprocess
import sys

repo_root = Path(sys.argv[1])
patterns = {
    "aws_access_key": re.compile(r"AKIA[0-9A-Z]{16}"),
    "github_classic_token": re.compile(r"gh[pousr]_[A-Za-z0-9]{20,}"),
    "github_pat": re.compile(r"github_pat_[A-Za-z0-9_]{20,}"),
    "slack_token": re.compile(r"xox[baprs]-[A-Za-z0-9-]{10,}"),
    "openai_key": re.compile(r"sk-[A-Za-z0-9]{24,}"),
    "stripe_live": re.compile(r"sk_live_[A-Za-z0-9]{16,}"),
    "private_key": re.compile(r"BEGIN (?:RSA|OPENSSH|EC|DSA|PGP) PRIVATE KEY"),
}

def allowlisted(rel: str, line: str) -> bool:
    if rel.startswith("crates/ralph/tests/"):
        return True
    if rel == "docs/features/security.md":
        return True
    if rel == "scripts/pre-public-check.sh" and "PRIVATE KEY" in line:
        return True
    if rel == "crates/ralph/src/fsutil.rs" and "AKIA" in line and "EXAMPLE" in line:
        return True
    if rel == "crates/ralph/src/fsutil.rs" and "OPENSSH PRIVATE KEY" in line:
        return True
    return False

paths = subprocess.run(
    ["git", "-C", str(repo_root), "ls-files", "-z", "--cached", "--others", "--exclude-standard"],
    check=True,
    capture_output=True,
).stdout.split(b"\0")

problems = []
for raw in paths:
    if not raw:
        continue
    rel = raw.decode("utf-8", errors="replace")
    path = repo_root / rel
    try:
        data = path.read_bytes()
    except OSError:
        continue
    if b"\0" in data:
        continue
    try:
        text = data.decode("utf-8")
    except UnicodeDecodeError:
        continue
    for line_number, line in enumerate(text.splitlines(), start=1):
        for name, pattern in patterns.items():
            match = pattern.search(line)
            if not match:
                continue
            if allowlisted(rel, line):
                continue
            problems.append(f"{rel}:{line_number}: {name}: {match.group(0)}")

if problems:
    for line in problems:
        print(line)
    sys.exit(1)
PY

    ralph_log_success "No obvious secret patterns found"
}

check_markdown_links() {
    if [ "$SKIP_LINKS" -eq 1 ]; then
        ralph_log_warn "Skipping markdown link checks"
        return 0
    fi

    ralph_log_info "Checking repo-wide markdown links"

    python3 - "$REPO_ROOT" <<'PY'
from pathlib import Path
import re
import subprocess
import sys

repo_root = Path(sys.argv[1])
paths = subprocess.run(
    ["git", "-C", str(repo_root), "ls-files", "-z", "--cached", "--others", "--exclude-standard", "--", "*.md"],
    check=True,
    capture_output=True,
).stdout.split(b"\0")
pattern = re.compile(r'!?\[[^\]]*\]\(([^)]+)\)')
missing = []
for raw in paths:
    if not raw:
        continue
    source = repo_root / raw.decode("utf-8", errors="replace")
    text = source.read_text(encoding="utf-8")
    for raw_target in pattern.findall(text):
        target = raw_target.strip().split()[0].strip('<>')
        if target.startswith(("http://", "https://", "mailto:", "#")):
            continue
        if "{{" in target or "}}" in target:
            continue
        target = target.split("#", 1)[0].split("?", 1)[0]
        if not target:
            continue
        resolved = (source.parent / target).resolve()
        if not resolved.exists():
            missing.append(f"{source.relative_to(repo_root)}: missing target -> {raw_target}")
if missing:
    for line in missing:
        print(line)
    sys.exit(1)
PY

    ralph_log_success "Markdown links look valid"
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
