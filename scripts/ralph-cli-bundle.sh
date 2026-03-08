#!/usr/bin/env bash
#
# Purpose: Build or resolve the canonical Ralph CLI binary for app bundling.
# Responsibilities:
# - Select the correct Cargo profile for Debug/Release consumers.
# - Reuse the pinned rustup toolchain when available.
# - Reuse explicit `RALPH_BIN_PATH` overrides when the caller already built the binary.
# - Print the binary path and optionally copy it into an app bundle destination.
# Scope:
# - CLI binary preparation only; Xcode and Makefile invoke this as the single bundling entrypoint.
# Usage:
# - scripts/ralph-cli-bundle.sh --configuration Release --print-path
# - scripts/ralph-cli-bundle.sh --configuration Debug --bundle-dir /path/to/Contents/MacOS
# Invariants/assumptions:
# - Cargo and the Ralph workspace are available locally.
# - The output executable is always named `ralph`.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/lib/ralph-shell.sh"
REPO_ROOT="$(ralph_repo_root)"

CONFIGURATION=""
BUNDLE_DIR=""
PRINT_PATH=0

usage() {
    cat <<'EOF'
Usage:
  scripts/ralph-cli-bundle.sh --configuration Debug|Release [--print-path] [--bundle-dir DIR]

Options:
  --configuration  Xcode-style configuration name used to choose Cargo profile
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
        profile_dir="release"
        build_args+=(--release)
        ;;
    Debug)
        ;;
    *)
        ralph_log_error "Unsupported configuration: $CONFIGURATION"
        exit 2
        ;;
esac

binary_path="$REPO_ROOT/target/$profile_dir/ralph"
if [ -n "${RALPH_BIN_PATH:-}" ]; then
    binary_path="$RALPH_BIN_PATH"
fi

if [ ! -x "$binary_path" ]; then
    ralph_log_info "Building Ralph CLI for $CONFIGURATION"
    (
        cd "$REPO_ROOT"
        cargo build "${build_args[@]}"
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
