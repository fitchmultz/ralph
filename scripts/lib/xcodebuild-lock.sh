#!/usr/bin/env bash
#
# Purpose: Provide shared Xcode build lock helpers for Ralph's macOS Make targets.
# Responsibilities:
# - Serialize Xcode build/test entrypoints with owner metadata under the project lock path.
# - Detect and clear stale project-owned lock directories left by interrupted runs.
# - Keep wait/recovery logging consistent across macOS validation targets.
# Scope:
# - Lock acquisition/release only; callers still own target-specific cleanup and xcodebuild invocation.
# Usage:
# - source scripts/lib/xcodebuild-lock.sh
# - ralph_acquire_xcode_build_lock "$lock_dir" "macos-build"
# - ralph_release_xcode_build_lock "$lock_dir"
# Invariants/assumptions:
# - Callers source this file from a bash shell inside the Ralph repository.
# - Stale-lock removal is limited to clearly project-owned lock paths under target/tmp/locks/.
# - Owner-file gaps are expected only during interrupted acquisition and recover via age-based cleanup.

if [ -n "${RALPH_XCODE_BUILD_LOCK_LIB_SOURCED:-}" ]; then
    return 0
fi
RALPH_XCODE_BUILD_LOCK_LIB_SOURCED=1

RALPH_XCODE_BUILD_LOCK_OWNER_FILE_NAME="owner"
RALPH_XCODE_BUILD_LOCK_OWNERLESS_GRACE_SECONDS=10
RALPH_XCODE_LOCK_STALE_REASON=""

ralph_xcode_build_lock_owner_file() {
    printf '%s/%s\n' "$1" "$RALPH_XCODE_BUILD_LOCK_OWNER_FILE_NAME"
}

ralph_xcode_build_lock_read_field() {
    local owner_file="$1"
    local field_name="$2"

    sed -n "s/^${field_name}: //p" "$owner_file" 2>/dev/null | head -1
}

ralph_xcode_build_lock_pid_is_running() {
    local pid="$1"

    [[ "$pid" =~ ^[0-9]+$ ]] || return 1
    kill -0 "$pid" >/dev/null 2>&1 && return 0
    ps -p "$pid" -o pid= >/dev/null 2>&1
}

ralph_xcode_build_lock_mtime_epoch() {
    local path="$1"

    if stat -f %m "$path" >/dev/null 2>&1; then
        stat -f %m "$path"
    else
        stat -c %Y "$path"
    fi
}

ralph_xcode_build_lock_is_older_than() {
    local path="$1"
    local min_age_seconds="$2"
    local mtime_epoch
    local now_epoch

    mtime_epoch="$(ralph_xcode_build_lock_mtime_epoch "$path")" || return 1
    now_epoch="$(date +%s)"
    [ $((now_epoch - mtime_epoch)) -ge "$min_age_seconds" ]
}

ralph_xcode_build_lock_is_project_owned() {
    case "$1" in
        target/tmp/locks/*|./target/tmp/locks/*|*/target/tmp/locks/*)
            return 0
            ;;
        *)
            return 1
            ;;
    esac
}

ralph_xcode_build_lock_is_stale() {
    local lock_dir="$1"
    local owner_file
    local owner_pid

    RALPH_XCODE_LOCK_STALE_REASON=""
    [ -d "$lock_dir" ] || return 1

    owner_file="$(ralph_xcode_build_lock_owner_file "$lock_dir")"
    if [ ! -f "$owner_file" ]; then
        if ralph_xcode_build_lock_is_older_than "$lock_dir" "$RALPH_XCODE_BUILD_LOCK_OWNERLESS_GRACE_SECONDS"; then
            RALPH_XCODE_LOCK_STALE_REASON="missing owner metadata"
            return 0
        fi
        return 1
    fi

    owner_pid="$(ralph_xcode_build_lock_read_field "$owner_file" "pid")"

    if [ -n "$owner_pid" ] && ralph_xcode_build_lock_pid_is_running "$owner_pid"; then
        return 1
    fi

    if [ -n "$owner_pid" ]; then
        RALPH_XCODE_LOCK_STALE_REASON="owner pid ${owner_pid:-unknown} is no longer running"
        return 0
    fi

    if ralph_xcode_build_lock_is_older_than "$lock_dir" "$RALPH_XCODE_BUILD_LOCK_OWNERLESS_GRACE_SECONDS"; then
        RALPH_XCODE_LOCK_STALE_REASON="invalid owner metadata"
        return 0
    fi

    return 1
}

ralph_write_xcode_build_lock_owner() {
    local lock_dir="$1"
    local label="$2"
    local owner_file
    local temp_owner_file

    owner_file="$(ralph_xcode_build_lock_owner_file "$lock_dir")"
    temp_owner_file="${owner_file}.tmp.$$"

    cat >"$temp_owner_file" <<EOF
pid: $$
started_at: $(date -u +%Y-%m-%dT%H:%M:%SZ)
command: make ${label}
label: ${label}
EOF
    mv "$temp_owner_file" "$owner_file"
}

ralph_acquire_xcode_build_lock() {
    local lock_dir="$1"
    local label="$2"
    local wait_notified=0

    mkdir -p "$(dirname "$lock_dir")"

    while ! mkdir "$lock_dir" 2>/dev/null; do
        if ralph_xcode_build_lock_is_stale "$lock_dir"; then
            if ralph_xcode_build_lock_is_project_owned "$lock_dir"; then
                echo "→ Removing stale Xcode build lock: $lock_dir ($RALPH_XCODE_LOCK_STALE_REASON)"
                rm -rf "$lock_dir"
                continue
            fi
        fi

        if [ "$wait_notified" = "0" ]; then
            echo "→ Waiting for Xcode build lock: $lock_dir"
            wait_notified=1
        fi
        sleep 1
    done

    if ! ralph_write_xcode_build_lock_owner "$lock_dir" "$label"; then
        rmdir "$lock_dir" 2>/dev/null || true
        return 1
    fi
}

ralph_release_xcode_build_lock() {
    local lock_dir="$1"
    local owner_file

    owner_file="$(ralph_xcode_build_lock_owner_file "$lock_dir")"
    rm -f "$owner_file" 2>/dev/null || true
    rmdir "$lock_dir" 2>/dev/null || true
}
