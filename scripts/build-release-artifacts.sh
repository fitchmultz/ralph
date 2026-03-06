#!/usr/bin/env bash
#
# Purpose: Build and package Ralph release artifacts for supported platforms.
# Responsibilities:
# - Build release binaries (native or cross target) with locked dependencies.
# - Produce tarball artifacts and SHA256 checksums under target/release-artifacts.
# Scope:
# - Artifact packaging only; no tagging/release publication.
# Usage:
# - scripts/build-release-artifacts.sh [--current|--all] [version]
# Invariants/assumptions:
# - Cargo and Rust toolchain are installed.
# - Cross targets must already be installed for non-native builds.

set -euo pipefail

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Configuration
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
source "$SCRIPT_DIR/versioning.sh"
RELEASE_ARTIFACTS_DIR="$REPO_ROOT/target/release-artifacts"

# Version (from argument or Cargo.toml)
VERSION="${1:-}"

# Logging functions
log_info() {
    echo -e "${BLUE}ℹ${NC} $1"
}

log_success() {
    echo -e "${GREEN}✓${NC} $1"
}

log_warn() {
    echo -e "${YELLOW}⚠${NC} $1"
}

log_error() {
    echo -e "${RED}✗${NC} $1"
}

log_step() {
    echo ""
    echo -e "${BLUE}▶${NC} $1"
    echo ""
}

sha256_file() {
    local file="$1"
    if command -v shasum >/dev/null 2>&1; then
        shasum -a 256 "$file"
    elif command -v sha256sum >/dev/null 2>&1; then
        sha256sum "$file"
    else
        log_error "No SHA256 checksum tool found (expected shasum or sha256sum)"
        exit 1
    fi
}

# Detect current platform
get_current_target() {
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

    log_error "Failed to detect rustc host target"
    exit 1
}

# Map target triple to platform name
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

# Check if a target is installed
check_target_installed() {
    local target="$1"
    rustup target list --installed 2>/dev/null | grep -q "^$target$" || return 1
}

# Install a target if not already installed
ensure_target() {
    local target="$1"
    if ! check_target_installed "$target"; then
        log_info "Installing target: $target"
        rustup target add "$target"
    fi
}

# Build for a specific target
build_for_target() {
    local target="$1"
    local version="$2"
    local platform_name
    platform_name=$(target_to_platform "$target")

    log_info "Building for $target ($platform_name)..."

    cd "$REPO_ROOT"

    # Build release binary
    if cargo build --release -p ralph-cli --target "$target" --locked --quiet; then
        log_success "Build successful for $target"
    else
        log_error "Build failed for $target"
        return 1
    fi

    # Create tarball
    local binary_path="$REPO_ROOT/target/$target/release/ralph"
    local tarball_name="ralph-${version}-${platform_name}.tar.gz"

    if [ -f "$binary_path" ]; then
        tar -czf "$RELEASE_ARTIFACTS_DIR/$tarball_name" -C "$REPO_ROOT/target/$target/release" ralph
        log_success "Created $tarball_name"

        # Generate checksum
        cd "$RELEASE_ARTIFACTS_DIR"
        sha256_file "$tarball_name" > "$tarball_name.sha256"
        log_success "Generated SHA256 checksum for $tarball_name"
    else
        log_error "Binary not found at $binary_path"
        return 1
    fi
}

# Build for current platform only
build_current() {
    local version="$1"
    local current_target
    current_target=$(get_current_target)

    log_step "Building for current platform: $current_target"

    cd "$REPO_ROOT"

    # Build release binary
    cargo build --release -p ralph-cli --locked --quiet

    # Create tarball
    local binary_path="$REPO_ROOT/target/release/ralph"
    local platform_name
    platform_name=$(target_to_platform "$current_target")
    local tarball_name="ralph-${version}-${platform_name}.tar.gz"

    mkdir -p "$RELEASE_ARTIFACTS_DIR"

    if [ -f "$binary_path" ]; then
        tar -czf "$RELEASE_ARTIFACTS_DIR/$tarball_name" -C "$REPO_ROOT/target/release" ralph
        log_success "Created $tarball_name"

        # Generate checksum
        cd "$RELEASE_ARTIFACTS_DIR"
        sha256_file "$tarball_name" > "$tarball_name.sha256"
        log_success "Generated SHA256 checksum"
    else
        log_error "Binary not found at $binary_path"
        exit 1
    fi
}

# Build for all supported platforms
build_all() {
    local version="$1"

    log_step "Building for all supported platforms"

    # Define targets
    local targets=(
        "x86_64-unknown-linux-gnu"
        "x86_64-apple-darwin"
        "aarch64-apple-darwin"
    )

    # Create artifacts directory
    mkdir -p "$RELEASE_ARTIFACTS_DIR"

    # Track failures
    local failed_targets=()

    for target in "${targets[@]}"; do
        echo ""
        log_info "Processing target: $target"

        # Check if we can build for this target
        if [ "$target" = "$(get_current_target)" ]; then
            # Native build
            if ! build_for_target "$target" "$version"; then
                failed_targets+=("$target")
            fi
        else
            # Cross-compilation
            if check_target_installed "$target" 2>/dev/null; then
                if ! build_for_target "$target" "$version"; then
                    failed_targets+=("$target")
                fi
            else
                log_warn "Target $target not installed, skipping"
                log_info "To build for this target, run: rustup target add $target"
                failed_targets+=("$target (not installed)")
            fi
        fi
    done

    # Report results
    echo ""
    log_step "Build Summary"

    if [ ${#failed_targets[@]} -eq 0 ]; then
        log_success "All builds completed successfully"
    else
        log_warn "Some builds failed or were skipped:"
        for target in "${failed_targets[@]}"; do
            echo "  - $target"
        done
    fi
}

# Print usage
print_usage() {
    cat << EOF
Usage: scripts/build-release-artifacts.sh [OPTIONS] [VERSION]

Build release artifacts for Ralph.

Arguments:
  VERSION    Version string (e.g., 0.2.0). If not provided, reads from VERSION

Options:
  --current  Build only for the current platform (default)
  --all      Build for all supported platforms (requires cross-compilation targets)
  --help     Show this help message

Exit codes:
  0  Success
  1  Runtime or unexpected failure
  2  Usage/validation error

Examples:
  scripts/build-release-artifacts.sh              # Build current platform, auto-detect version
  scripts/build-release-artifacts.sh 0.2.0        # Build current platform with specific version
  scripts/build-release-artifacts.sh --all 0.2.0  # Build all platforms

Supported Platforms:
  - x86_64-unknown-linux-gnu (Linux x64)
  - x86_64-apple-darwin (macOS x64)
  - aarch64-apple-darwin (macOS ARM64)

EOF
}

# Main function
main() {
    local build_mode="current"

    # Parse arguments
    while [[ $# -gt 0 ]]; do
        case "$1" in
            --current)
                build_mode="current"
                shift
                ;;
            --all)
                build_mode="all"
                shift
                ;;
            --help|-h)
                print_usage
                exit 0
                ;;
            -*)
                log_error "Unknown option: $1"
                print_usage
                exit 2
                ;;
            *)
                VERSION="$1"
                shift
                ;;
        esac
    done

    # Get version if not provided
    if [ -z "$VERSION" ]; then
        VERSION=$(read_canonical_version)
        log_info "Using version from VERSION: $VERSION"
    fi

    # Validate version
    if ! validate_semver "$VERSION"; then
        log_error "Invalid version format: $VERSION"
        echo "  Version must be in semver format (e.g., 0.2.0)"
        exit 2
    fi

    echo "═══════════════════════════════════════════════════"
    echo -e "  ${GREEN}BUILD RELEASE ARTIFACTS${NC}"
    echo "═══════════════════════════════════════════════════"
    echo "  Version: $VERSION"
    echo "  Mode: $build_mode"
    echo "═══════════════════════════════════════════════════"
    echo ""

    # Create artifacts directory
    mkdir -p "$RELEASE_ARTIFACTS_DIR"

    # Build based on mode
    case "$build_mode" in
        current)
            build_current "$VERSION"
            ;;
        all)
            build_all "$VERSION"
            ;;
    esac

    # List artifacts
    echo ""
    log_step "Artifacts"
    if [ -d "$RELEASE_ARTIFACTS_DIR" ]; then
        ls -lh "$RELEASE_ARTIFACTS_DIR"
    fi

    echo ""
    echo "═══════════════════════════════════════════════════"
    log_success "Build complete"
    echo "  Artifacts: $RELEASE_ARTIFACTS_DIR"
    echo "═══════════════════════════════════════════════════"
}

main "$@"
