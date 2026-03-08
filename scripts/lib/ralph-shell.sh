#!/usr/bin/env bash
#
# Purpose: Provide shared shell utilities for Ralph maintenance scripts.
# Responsibilities:
# - Resolve the repository root and shared filesystem helpers.
# - Standardize logging, semver validation, checksum helpers, and make/rust lookup.
# - Activate the pinned rustup toolchain when available.
# Scope:
# - Common shell behavior only; release/public-readiness policy lives elsewhere.
# Usage:
# - source "$(dirname "$0")/lib/ralph-shell.sh"
# Invariants/assumptions:
# - Caller defines SCRIPT_DIR before sourcing this file.
# - Scripts source this helper from within the Ralph repository.

if [ -n "${RALPH_SHELL_LIB_SOURCED:-}" ]; then
    return 0
fi
RALPH_SHELL_LIB_SOURCED=1

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

ralph_repo_root() {
    cd "${SCRIPT_DIR}/.." >/dev/null 2>&1
    pwd
}

ralph_log_info() {
    echo -e "${BLUE}ℹ${NC} $1"
}

ralph_log_success() {
    echo -e "${GREEN}✓${NC} $1"
}

ralph_log_warn() {
    echo -e "${YELLOW}⚠${NC} $1"
}

ralph_log_error() {
    echo -e "${RED}✗${NC} $1" >&2
}

ralph_log_step() {
    echo ""
    echo -e "${BLUE}▶${NC} $1"
    echo ""
}

ralph_validate_semver() {
    local version="$1"
    [[ "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]
}

ralph_mktemp_file() {
    local prefix="$1"
    local base="${TMPDIR:-/tmp}"
    base="${base%/}"
    mktemp "${base}/${prefix}.XXXXXX"
}

ralph_sha256_file() {
    local file="$1"
    if command -v shasum >/dev/null 2>&1; then
        shasum -a 256 "$file"
    elif command -v sha256sum >/dev/null 2>&1; then
        sha256sum "$file"
    else
        ralph_log_error "No SHA256 checksum tool found (expected shasum or sha256sum)"
        return 1
    fi
}

ralph_resolve_make_cmd() {
    if [ -n "${RALPH_MAKE_CMD:-}" ]; then
        echo "$RALPH_MAKE_CMD"
        return 0
    fi

    if command -v gmake >/dev/null 2>&1; then
        echo "gmake"
        return 0
    fi

    if command -v make >/dev/null 2>&1 && make --version 2>/dev/null | grep -q "GNU Make"; then
        echo "make"
        return 0
    fi

    ralph_log_error "GNU Make is required (install with 'brew install make' and use gmake)."
    return 1
}

ralph_get_repo_http_url() {
    local remote_url
    remote_url=$(git -C "$REPO_ROOT" remote get-url origin 2>/dev/null || true)
    if [ -z "$remote_url" ]; then
        ralph_log_error "Failed to resolve git remote 'origin' URL"
        return 1
    fi

    case "$remote_url" in
        https://github.com/*.git)
            printf '%s\n' "${remote_url%.git}"
            ;;
        https://github.com/*)
            printf '%s\n' "$remote_url"
            ;;
        git@github.com:*.git)
            remote_url="${remote_url#git@github.com:}"
            remote_url="${remote_url%.git}"
            printf 'https://github.com/%s\n' "$remote_url"
            ;;
        git@github.com:*)
            remote_url="${remote_url#git@github.com:}"
            printf 'https://github.com/%s\n' "$remote_url"
            ;;
        *)
            ralph_log_error "Unsupported origin remote URL format: $remote_url"
            return 1
            ;;
    esac
}

ralph_get_rust_host_target() {
    local host
    host=$(rustc --print host-tuple 2>/dev/null || true)
    if [ -n "$host" ]; then
        echo "$host"
        return 0
    fi

    host=$(rustc --version --verbose 2>/dev/null | sed -n 's/^host: //p' | head -1 || true)
    if [ -n "$host" ]; then
        echo "$host"
        return 0
    fi

    ralph_log_error "Unable to determine rustc host target"
    return 1
}

ralph_activate_pinned_rust_toolchain() {
    local toolchain_file="$REPO_ROOT/rust-toolchain.toml"
    if [ ! -f "$toolchain_file" ] || ! command -v rustup >/dev/null 2>&1; then
        return 0
    fi

    local toolchain
    toolchain=$(sed -n 's/^[[:space:]]*channel = "\(.*\)"/\1/p' "$toolchain_file" | head -1 || true)
    if [ -z "$toolchain" ]; then
        return 0
    fi

    local rustc_path
    rustc_path=$(rustup which rustc --toolchain "$toolchain" 2>/dev/null || true)
    if [ -z "$rustc_path" ]; then
        return 0
    fi

    local rust_bin_dir
    rust_bin_dir=$(dirname "$rustc_path")
    export PATH="${rust_bin_dir}:$PATH"
    export CARGO="${rust_bin_dir}/cargo"
    export RUSTC="$rustc_path"
}
