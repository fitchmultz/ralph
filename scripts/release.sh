#!/usr/bin/env bash
#
# Purpose: Execute Ralph releases as explicit verify/execute/reconcile transactions.
# Responsibilities:
# - Prepare and record a publish-ready local release snapshot before any remote mutation.
# - Publish only from a previously verified release snapshot.
# - Resume partially completed remote publication from recorded transaction state.
# Scope:
# - Local release automation only; no remote CI or GitHub Actions.
# Usage:
# - scripts/release.sh verify <version>
# - scripts/release.sh execute <version>
# - scripts/release.sh reconcile <version>
# Invariants/assumptions:
# - Version must be strict semver (x.y.z).
# - `verify` starts from a clean `main` worktree and records the publish-ready snapshot.
# - `execute` publishes only after the verified snapshot matches the current workspace.
# - Transaction state lives under `target/release-transactions/v<version>/`.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/lib/ralph-shell.sh"
REPO_ROOT="$(ralph_repo_root)"
source "$SCRIPT_DIR/versioning.sh"
source "$SCRIPT_DIR/lib/release_policy.sh"
source "$SCRIPT_DIR/lib/release_state.sh"
source "$SCRIPT_DIR/lib/release_verify_state.sh"
source "$SCRIPT_DIR/lib/release_changelog.sh"
source "$SCRIPT_DIR/lib/release_pipeline.sh"

CHANGELOG="$REPO_ROOT/CHANGELOG.md"
RELEASE_NOTES_TEMPLATE="$REPO_ROOT/.github/release-notes-template.md"
RELEASE_ARTIFACTS_DIR="$REPO_ROOT/target/release-artifacts"
CRATE_PACKAGE_NAME="ralph-agent-loop"

COMMAND="${1:-}"
VERSION="${2:-}"

usage() {
    cat <<'EOF'
Ralph release transaction workflow.

Usage:
  scripts/release.sh verify <version>
  scripts/release.sh execute <version>
  scripts/release.sh reconcile <version>
  scripts/release.sh --help

Commands:
  verify     Prepare and record a publish-ready local snapshot without remote publication
  execute    Publish the previously verified snapshot through the transaction pipeline
  reconcile  Resume a previously recorded transaction for the same version

Examples:
  scripts/release.sh verify 0.2.0
  scripts/release.sh execute 0.2.0
  scripts/release.sh reconcile 0.2.0

Exit codes:
  0  Success
  1  Runtime or unexpected failure
  2  Usage/validation error

Release model:
  1. verify prepares a local publish-ready snapshot (versions, checks, artifacts, notes)
  2. execute validates that exact snapshot still matches the workspace
  3. execute creates the release commit/tag and publishes remotely
  4. reconcile resumes from recorded transaction state if a remote step fails
EOF
}

print_execute_summary() {
    echo ""
    echo "═══════════════════════════════════════════════════"
    echo -e "  ${GREEN}RELEASE COMPLETE${NC}"
    echo "═══════════════════════════════════════════════════"
    echo "  Version: v$VERSION"
    echo "  Transaction: $TRANSACTION_DIR"
    echo ""
    echo "  Verify:"
    echo "    cargo install $CRATE_PACKAGE_NAME"
    echo "    gh release view v$VERSION"
    echo "═══════════════════════════════════════════════════"
}

print_reconcile_hint() {
    echo ""
    ralph_log_warn "Release transaction recorded for recovery"
    echo "  Transaction: $TRANSACTION_DIR"
    echo "  Resume with: scripts/release.sh reconcile $VERSION"
}

run_verify() {
    release_check_prerequisites 0
    release_validate_repo_state 0
    release_verify_plan
    release_verify_state_init
    release_prepare_verified_snapshot
    ralph_log_success "Release snapshot prepared for v$VERSION"
}

run_execute() {
    if ! release_check_prerequisites 1 || ! release_validate_repo_state 0 1 || ! release_verify_state_load || ! release_verify_assert_ready_for_execute; then
        return 1
    fi
    release_state_init "execute"
    REPO_HTTP_URL="$VERIFY_REPO_HTTP_URL"
    RELEASE_NOTES_FILE="$VERIFY_RELEASE_NOTES_FILE"
    release_state_write

    if ! release_publish_crate || ! release_push_remote_state || ! release_create_github_release; then
        print_reconcile_hint
        return 1
    fi

    print_execute_summary
}

run_reconcile() {
    TRANSACTION_DIR="$REPO_ROOT/target/release-transactions/v$VERSION"
    STATE_FILE="$TRANSACTION_DIR/state.env"
    VERIFY_DIR="$RELEASE_VERIFY_DIR_ROOT/v$VERSION"
    VERIFY_STATE_FILE="$VERIFY_DIR/state.env"
    release_state_load
    release_check_prerequisites 1

    if ! release_publish_crate || ! release_push_remote_state || ! release_create_github_release; then
        print_reconcile_hint
        return 1
    fi

    print_execute_summary
}

main() {
    if [ "$COMMAND" = "--help" ] || [ "$COMMAND" = "-h" ]; then
        usage
        exit 0
    fi

    if [ -z "$COMMAND" ]; then
        ralph_log_error "VERSION is required"
        usage
        exit 2
    fi

    if [ -z "$VERSION" ]; then
        ralph_log_error "VERSION is required"
        usage
        exit 2
    fi

    if ! ralph_validate_semver "$VERSION"; then
        ralph_log_error "VERSION must be in semver format (e.g. 0.2.0)"
        exit 2
    fi

    TRANSACTION_DIR="$REPO_ROOT/target/release-transactions/v$VERSION"
    STATE_FILE="$TRANSACTION_DIR/state.env"
    VERIFY_DIR="$RELEASE_VERIFY_DIR_ROOT/v$VERSION"
    VERIFY_STATE_FILE="$VERIFY_DIR/state.env"
    RELEASE_NOTES_FILE="$REPO_ROOT/target/release-notes-v$VERSION.md"

    case "$COMMAND" in
        verify)
            run_verify
            ;;
        execute)
            run_execute
            ;;
        reconcile)
            run_reconcile
            ;;
        *)
            ralph_log_error "Unknown command: $COMMAND"
            usage
            exit 2
            ;;
    esac
}

main "$@"
