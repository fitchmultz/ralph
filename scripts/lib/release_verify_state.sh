#!/usr/bin/env bash
#
# Purpose: Persist and validate publish-ready release verification snapshots.
# Responsibilities:
# - Record the exact local release state prepared by `scripts/release.sh verify`.
# - Capture manifests for release metadata, release notes, and packaged artifacts.
# - Enforce that `scripts/release.sh execute` only publishes the verified snapshot it was given.
# Scope:
# - Verification snapshot bookkeeping only; it does not mutate git refs or publish remotely.
# Usage:
# - source "$(dirname "$0")/lib/release_verify_state.sh"
# Invariants/assumptions:
# - VERSION is a validated semver string before initialization.
# - Verified snapshots live under `target/release-verifications/v<version>/`.
# - VERIFY_STATE_FILE points at an env-style file owned by the verification snapshot.

if [ -n "${RALPH_RELEASE_VERIFY_STATE_SOURCED:-}" ]; then
    return 0
fi
RALPH_RELEASE_VERIFY_STATE_SOURCED=1

release_verify_state_reset_vars() {
    VERIFY_STATUS="${VERIFY_STATUS:-initialized}"
    VERIFY_SOURCE_COMMIT="${VERIFY_SOURCE_COMMIT:-}"
    VERIFY_REPO_HTTP_URL="${VERIFY_REPO_HTTP_URL:-}"
    VERIFY_RELEASE_NOTES_FILE="${VERIFY_RELEASE_NOTES_FILE:-$REPO_ROOT/target/release-notes-v$VERSION.md}"
    VERIFY_METADATA_MANIFEST="${VERIFY_METADATA_MANIFEST:-$VERIFY_DIR/metadata.sha256}"
    VERIFY_ARTIFACT_MANIFEST="${VERIFY_ARTIFACT_MANIFEST:-$VERIFY_DIR/artifacts.sha256}"
    VERIFY_DIR="${VERIFY_DIR:-$RELEASE_VERIFY_DIR_ROOT/v$VERSION}"
    VERIFIED_AT="${VERIFIED_AT:-$(date -u +%Y-%m-%dT%H:%M:%SZ)}"
}

release_verify_state_write() {
    release_verify_state_reset_vars
    mkdir -p "$VERIFY_DIR"
    cat > "$VERIFY_STATE_FILE" <<EOF
VERSION=$VERSION
VERIFY_STATUS=$VERIFY_STATUS
VERIFY_SOURCE_COMMIT=$VERIFY_SOURCE_COMMIT
VERIFY_REPO_HTTP_URL=$VERIFY_REPO_HTTP_URL
VERIFY_RELEASE_NOTES_FILE=$VERIFY_RELEASE_NOTES_FILE
VERIFY_METADATA_MANIFEST=$VERIFY_METADATA_MANIFEST
VERIFY_ARTIFACT_MANIFEST=$VERIFY_ARTIFACT_MANIFEST
VERIFY_DIR=$VERIFY_DIR
VERIFIED_AT=$VERIFIED_AT
EOF
}

release_verify_state_load() {
    if [ ! -f "$VERIFY_STATE_FILE" ]; then
        ralph_log_error "Verified release snapshot not found: $VERIFY_STATE_FILE"
        echo "  Run: scripts/release.sh verify $VERSION" >&2
        return 1
    fi

    # shellcheck disable=SC1090
    source "$VERIFY_STATE_FILE"
    release_verify_state_reset_vars
}

release_verify_state_init() {
    VERIFY_DIR="$RELEASE_VERIFY_DIR_ROOT/v$VERSION"
    VERIFY_STATE_FILE="$VERIFY_DIR/state.env"

    if [ -e "$STATE_FILE" ]; then
        ralph_log_error "Release transaction already exists for v$VERSION"
        echo "  Continue it with: scripts/release.sh reconcile $VERSION" >&2
        return 1
    fi

    rm -rf "$VERIFY_DIR"
    mkdir -p "$VERIFY_DIR"

    VERIFY_STATUS="initialized"
    VERIFY_SOURCE_COMMIT="$(git -C "$REPO_ROOT" rev-parse HEAD)"
    VERIFY_REPO_HTTP_URL=""
    VERIFY_RELEASE_NOTES_FILE="$REPO_ROOT/target/release-notes-v$VERSION.md"
    VERIFY_METADATA_MANIFEST="$VERIFY_DIR/metadata.sha256"
    VERIFY_ARTIFACT_MANIFEST="$VERIFY_DIR/artifacts.sha256"
    VERIFIED_AT="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
    release_verify_state_write
}

release_verify_write_metadata_manifest() {
    : > "$VERIFY_METADATA_MANIFEST"
    local relative_path
    for relative_path in "${RELEASE_METADATA_PATHS[@]}"; do
        ralph_sha256_file "$REPO_ROOT/$relative_path" >> "$VERIFY_METADATA_MANIFEST"
    done
}

release_verify_write_artifact_manifest() {
    : > "$VERIFY_ARTIFACT_MANIFEST"

    if [ -f "$VERIFY_RELEASE_NOTES_FILE" ]; then
        ralph_sha256_file "$VERIFY_RELEASE_NOTES_FILE" >> "$VERIFY_ARTIFACT_MANIFEST"
    fi

    if [ -d "$RELEASE_ARTIFACTS_DIR" ]; then
        while IFS= read -r artifact; do
            [ -n "$artifact" ] || continue
            ralph_sha256_file "$artifact" >> "$VERIFY_ARTIFACT_MANIFEST"
        done < <(find "$RELEASE_ARTIFACTS_DIR" -maxdepth 1 -type f | LC_ALL=C sort)
    fi
}

release_verify_record_ready_snapshot() {
    VERIFY_STATUS="ready"
    VERIFY_REPO_HTTP_URL="${REPO_HTTP_URL:-}"
    VERIFY_RELEASE_NOTES_FILE="${RELEASE_NOTES_FILE:-$VERIFY_RELEASE_NOTES_FILE}"
    release_verify_write_metadata_manifest
    release_verify_write_artifact_manifest
    VERIFIED_AT="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
    release_verify_state_write
}

release_verify_assert_manifest_matches() {
    local manifest_path="$1"
    local label="$2"

    if [ ! -f "$manifest_path" ]; then
        ralph_log_error "Missing $label manifest: $manifest_path"
        return 1
    fi

    local line
    local expected_hash
    local file_path
    local actual_hash
    while IFS= read -r line; do
        [ -z "$line" ] && continue
        expected_hash=$(printf '%s\n' "$line" | awk '{print $1}')
        file_path=$(printf '%s\n' "$line" | cut -d' ' -f3-)
        if [ ! -f "$file_path" ]; then
            ralph_log_error "Verified snapshot is missing $label file: $file_path"
            return 1
        fi
        actual_hash=$(ralph_sha256_file "$file_path" | awk '{print $1}')
        if [ "$actual_hash" != "$expected_hash" ]; then
            ralph_log_error "Verified snapshot drifted for $file_path"
            return 1
        fi
    done < "$manifest_path"
}

release_verify_assert_ready_for_execute() {
    if [ "$VERIFY_STATUS" != "ready" ]; then
        ralph_log_error "Verified release snapshot is not ready for publish (status: $VERIFY_STATUS)"
        return 1
    fi

    local current_head
    current_head=$(git -C "$REPO_ROOT" rev-parse HEAD)
    if [ "$current_head" != "$VERIFY_SOURCE_COMMIT" ]; then
        ralph_log_error "HEAD moved since release verification"
        echo "  Verified commit: $VERIFY_SOURCE_COMMIT" >&2
        echo "  Current commit:  $current_head" >&2
        echo "  Re-run: scripts/release.sh verify $VERSION" >&2
        return 1
    fi

    if ! release_verify_assert_manifest_matches "$VERIFY_METADATA_MANIFEST" "release-metadata"; then
        return 1
    fi

    if ! release_verify_assert_manifest_matches "$VERIFY_ARTIFACT_MANIFEST" "release-artifact"; then
        return 1
    fi

    ralph_log_success "Verified release snapshot matches current workspace"
}
