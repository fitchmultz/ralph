#!/usr/bin/env bash
#
# Purpose: Persist release transaction state for execute/reconcile flows.
# Responsibilities:
# - Initialize, load, and update release transaction state files under target/.
# - Persist enough state to resume publication after partial remote failures.
# Scope:
# - Transaction bookkeeping only; it does not perform git, cargo, or gh operations.
# Usage:
# - source "$(dirname "$0")/lib/release_state.sh"
# Invariants/assumptions:
# - VERSION is a validated semver string before transaction initialization.
# - STATE_FILE points at an env-style file owned by the release transaction.

if [ -n "${RALPH_RELEASE_STATE_SOURCED:-}" ]; then
    return 0
fi
RALPH_RELEASE_STATE_SOURCED=1

release_state_reset_vars() {
    RELEASE_STATUS="${RELEASE_STATUS:-initialized}"
    RELEASE_MODE="${RELEASE_MODE:-execute}"
    RELEASE_COMMIT="${RELEASE_COMMIT:-}"
    LOCAL_TAG_CREATED="${LOCAL_TAG_CREATED:-0}"
    CRATE_PUBLISHED="${CRATE_PUBLISHED:-0}"
    REMOTE_PUSHED="${REMOTE_PUSHED:-0}"
    GITHUB_RELEASE_CREATED="${GITHUB_RELEASE_CREATED:-0}"
    REPO_HTTP_URL="${REPO_HTTP_URL:-}"
    RELEASE_NOTES_FILE="${RELEASE_NOTES_FILE:-$REPO_ROOT/target/release-notes-v$VERSION.md}"
    TRANSACTION_DIR="${TRANSACTION_DIR:-$REPO_ROOT/target/release-transactions/v$VERSION}"
    STARTED_AT="${STARTED_AT:-$(date -u +%Y-%m-%dT%H:%M:%SZ)}"
}

release_state_write() {
    release_state_reset_vars
    mkdir -p "$TRANSACTION_DIR"
    cat > "$STATE_FILE" <<EOF
VERSION=$VERSION
RELEASE_MODE=$RELEASE_MODE
RELEASE_STATUS=$RELEASE_STATUS
RELEASE_COMMIT=$RELEASE_COMMIT
LOCAL_TAG_CREATED=$LOCAL_TAG_CREATED
CRATE_PUBLISHED=$CRATE_PUBLISHED
REMOTE_PUSHED=$REMOTE_PUSHED
GITHUB_RELEASE_CREATED=$GITHUB_RELEASE_CREATED
REPO_HTTP_URL=$REPO_HTTP_URL
RELEASE_NOTES_FILE=$RELEASE_NOTES_FILE
TRANSACTION_DIR=$TRANSACTION_DIR
STARTED_AT=$STARTED_AT
EOF
}

release_state_load() {
    if [ ! -f "$STATE_FILE" ]; then
        ralph_log_error "Release transaction state not found: $STATE_FILE"
        return 1
    fi

    # shellcheck disable=SC1090
    source "$STATE_FILE"
    release_state_reset_vars
}

release_state_init() {
    TRANSACTION_DIR="$REPO_ROOT/target/release-transactions/v$VERSION"
    STATE_FILE="$TRANSACTION_DIR/state.env"
    if [ -e "$STATE_FILE" ]; then
        ralph_log_error "Release transaction already exists for v$VERSION"
        echo "  Continue it with: scripts/release.sh reconcile $VERSION" >&2
        return 1
    fi

    RELEASE_MODE="$1"
    RELEASE_STATUS="initialized"
    RELEASE_COMMIT=""
    LOCAL_TAG_CREATED=0
    CRATE_PUBLISHED=0
    REMOTE_PUSHED=0
    GITHUB_RELEASE_CREATED=0
    REPO_HTTP_URL=""
    RELEASE_NOTES_FILE="$REPO_ROOT/target/release-notes-v$VERSION.md"
    STARTED_AT="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
    release_state_write
}
