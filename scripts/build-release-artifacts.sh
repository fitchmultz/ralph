#!/usr/bin/env bash
#
# Purpose: Build and package Ralph release artifacts for supported platforms.
# Responsibilities:
# - Build the canonical CLI binary through the shared bundling entrypoint for native artifacts.
# - Build optional cross-target artifacts with locked Cargo dependencies.
# - Produce tarballs and SHA256 checksums under target/release-artifacts.
# Scope:
# - Artifact packaging only; release publication happens elsewhere.
# Usage:
# - scripts/build-release-artifacts.sh [--current|--all] [version]
# Invariants/assumptions:
# - Cargo and Rust toolchain are installed.
# - Cross targets must already be installed for non-native builds.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/lib/ralph-shell.sh"
REPO_ROOT="$(ralph_repo_root)"
source "$SCRIPT_DIR/versioning.sh"

RELEASE_ARTIFACTS_DIR="$REPO_ROOT/target/release-artifacts"
MODE="current"
VERSION=""

target_to_platform() {
    local target="$1"
    case "$target" in
        x86_64-unknown-linux-gnu|x86_64-unknown-linux-musl)
            echo "linux-x64"
            ;;
        x86_64-apple-darwin)
            echo "macos-x64"
            ;;
        aarch64-apple-darwin)
            echo "macos-arm64"
            ;;
        *)
            echo "$target"
            ;;
    esac
}

build_native_release_artifact() {
    local version="$1"
    local binary_path
    binary_path=$("$SCRIPT_DIR/ralph-cli-bundle.sh" --configuration Release --print-path)
    local target_triple
    target_triple=$(ralph_get_rust_host_target)
    local platform_name
    platform_name=$(target_to_platform "$target_triple")
    local tarball_name="ralph-${version}-${platform_name}.tar.gz"

    mkdir -p "$RELEASE_ARTIFACTS_DIR"
    tar -czf "$RELEASE_ARTIFACTS_DIR/$tarball_name" -C "$(dirname "$binary_path")" ralph
    (
        cd "$RELEASE_ARTIFACTS_DIR"
        ralph_sha256_file "$tarball_name" > "$tarball_name.sha256"
    )
}

build_cross_target() {
    local target="$1"
    local version="$2"
    local platform_name
    platform_name=$(target_to_platform "$target")
    local binary_path="$REPO_ROOT/target/$target/release/ralph"
    local tarball_name="ralph-${version}-${platform_name}.tar.gz"

    (
        cd "$REPO_ROOT"
        cargo build --release -p ralph-agent-loop --target "$target" --locked
    )
    tar -czf "$RELEASE_ARTIFACTS_DIR/$tarball_name" -C "$REPO_ROOT/target/$target/release" ralph
    (
        cd "$RELEASE_ARTIFACTS_DIR"
        ralph_sha256_file "$tarball_name" > "$tarball_name.sha256"
    )
}

usage() {
    cat <<'EOF'
Usage: scripts/build-release-artifacts.sh [OPTIONS] [VERSION]

Options:
  --current   Build only the current host artifact (default)
  --all       Build all supported artifacts
  -h, --help  Show this help
EOF
}

while [ $# -gt 0 ]; do
    case "$1" in
        --current)
            MODE="current"
            ;;
        --all)
            MODE="all"
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        *)
            if [ -n "$VERSION" ]; then
                ralph_log_error "Unexpected extra argument: $1"
                exit 2
            fi
            VERSION="$1"
            ;;
    esac
    shift
done

if [ -z "$VERSION" ]; then
    VERSION=$(read_canonical_version)
fi

if ! ralph_validate_semver "$VERSION"; then
    ralph_log_error "VERSION must be in semver format"
    exit 2
fi

rm -rf "$RELEASE_ARTIFACTS_DIR"
mkdir -p "$RELEASE_ARTIFACTS_DIR"
ralph_activate_pinned_rust_toolchain

if [ "$MODE" = "current" ]; then
    ralph_log_step "Building current-platform release artifact"
    build_native_release_artifact "$VERSION"
else
    ralph_log_step "Building all supported release artifacts"
    build_native_release_artifact "$VERSION"
    for target in x86_64-unknown-linux-gnu x86_64-apple-darwin aarch64-apple-darwin; do
        if [ "$target" = "$(ralph_get_rust_host_target)" ]; then
            continue
        fi
        if rustup target list --installed 2>/dev/null | grep -q "^$target$"; then
            build_cross_target "$target" "$VERSION"
        else
            ralph_log_warn "Skipping cross target not installed: $target"
        fi
    done
fi

ralph_log_success "Release artifacts are available in $RELEASE_ARTIFACTS_DIR"
