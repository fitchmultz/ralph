#!/usr/bin/env bash
#
# Purpose: Build or resolve the canonical Ralph CLI binary for app bundling.
# Responsibilities:
# - Select the correct Cargo profile for Debug/Release consumers.
# - Optionally build for an explicit Rust target triple and bounded job count.
# - Reuse the pinned rustup toolchain when available.
# - Print the canonical binary path and optionally copy it into an app bundle destination.
# Scope:
# - CLI binary preparation only; Xcode and Makefile invoke this as the single bundling entrypoint.
# Usage:
# - scripts/ralph-cli-bundle.sh --configuration Release --print-path
# - scripts/ralph-cli-bundle.sh --configuration Debug --bundle-dir /path/to/Contents/MacOS
# - scripts/ralph-cli-bundle.sh --configuration Release --target x86_64-unknown-linux-gnu --jobs 4 --print-path
# Invariants/assumptions:
# - Cargo and the Ralph workspace are available locally.
# - The output executable is always named `ralph`.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/lib/ralph-shell.sh"
REPO_ROOT="$(ralph_repo_root)"

CONFIGURATION=""
BUNDLE_DIR=""
TARGET_TRIPLE=""
JOBS=""
PRINT_PATH=0

input_paths() {
    printf '%s\n' \
        "$REPO_ROOT/Cargo.toml" \
        "$REPO_ROOT/Cargo.lock" \
        "$REPO_ROOT/VERSION" \
        "$REPO_ROOT/rust-toolchain.toml" \
        "$REPO_ROOT/scripts/ralph-cli-bundle.sh"
    if [ -d "$REPO_ROOT/.cargo" ]; then
        find "$REPO_ROOT/.cargo" -type f -print | LC_ALL=C sort
    fi
    if [ -d "$REPO_ROOT/crates" ]; then
        find "$REPO_ROOT/crates" -type f \( -name '*.rs' -o -name 'Cargo.toml' -o -name 'build.rs' \) -print | LC_ALL=C sort
    fi
}

binary_is_fresh() {
    [ -x "$binary_path" ] || return 1
    while IFS= read -r input_path; do
        [ -n "$input_path" ] || continue
        [ -e "$input_path" ] || continue
        if [ "$input_path" -nt "$binary_path" ]; then
            return 1
        fi
    done < <(input_paths)
    return 0
}

usage() {
    cat <<'EOF'
Usage:
  scripts/ralph-cli-bundle.sh --configuration Debug|Release [--target TRIPLE] [--jobs N] [--print-path] [--bundle-dir DIR]

Options:
  --configuration  Xcode-style configuration name used to choose Cargo profile
  --target         Optional Rust target triple for cross/native builds
  --jobs           Optional cargo build job cap
  --print-path     Print the resolved executable path to stdout
  --bundle-dir     Copy the resolved executable into DIR/ralph
  -h, --help       Show this help

Exit codes:
  0  Success
  1  Runtime or unexpected failure
  2  Usage/validation error
EOF
}

while [ $# -gt 0 ]; do
    case "$1" in
        --configuration)
            CONFIGURATION="${2:-}"
            shift
            ;;
        --bundle-dir)
            BUNDLE_DIR="${2:-}"
            shift
            ;;
        --target)
            TARGET_TRIPLE="${2:-}"
            shift
            ;;
        --jobs)
            JOBS="${2:-}"
            shift
            ;;
        --print-path)
            PRINT_PATH=1
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

if [ -z "$CONFIGURATION" ]; then
    ralph_log_error "--configuration is required"
    usage
    exit 2
fi

ralph_activate_pinned_rust_toolchain

profile_dir="debug"
build_args=(-p ralph-agent-loop --locked)
case "$CONFIGURATION" in
    Release)
        profile_dir="dist"
        build_args+=(--profile dist)
        ;;
    Debug)
        ;;
    *)
        ralph_log_error "Unsupported configuration: $CONFIGURATION"
        exit 2
        ;;
esac

target_root="${CARGO_TARGET_DIR:-$REPO_ROOT/target}"
case "$target_root" in
    /*) ;;
    *) target_root="$REPO_ROOT/$target_root" ;;
esac

if [ -n "$TARGET_TRIPLE" ]; then
    build_args+=(--target "$TARGET_TRIPLE")
    binary_path="$target_root/$TARGET_TRIPLE/$profile_dir/ralph"
else
    binary_path="$target_root/$profile_dir/ralph"
fi

if [ -n "$JOBS" ] && [ "$JOBS" != "0" ]; then
    build_args+=(--jobs "$JOBS")
fi

if binary_is_fresh; then
    ralph_log_info "Reusing fresh Ralph CLI for $CONFIGURATION" >&2
else
    ralph_log_info "Building Ralph CLI for $CONFIGURATION" >&2
    (
        cd "$REPO_ROOT"
        "${CARGO:-cargo}" build "${build_args[@]}"
    )
fi

if [ ! -x "$binary_path" ]; then
    ralph_log_error "Built CLI binary is missing: $binary_path"
    exit 1
fi

if [ -n "$BUNDLE_DIR" ]; then
    mkdir -p "$BUNDLE_DIR"
    cp -f "$binary_path" "$BUNDLE_DIR/ralph"
    chmod +x "$BUNDLE_DIR/ralph"
fi

if [ "$PRINT_PATH" = "1" ]; then
    printf '%s\n' "$binary_path"
fi
