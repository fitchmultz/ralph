#!/usr/bin/env bash
#
# Release script for Ralph - Local release workflow without GitHub Actions
#
# This script handles the complete release process:
# - Pre-release validation (clean working dir, main branch, CI passes)
# - Version bumping in Cargo.toml
# - CHANGELOG.md updates
# - Multi-platform release artifact builds
# - Checksum generation (SHA256)
# - Git commit and annotated tag creation
# - GitHub release creation with asset upload via gh CLI
#
# Usage:
#   scripts/release.sh <version>              # Full release
#   RELEASE_DRY_RUN=1 scripts/release.sh <version>  # Dry run (no side effects)
#
# Requirements:
#   - gh CLI installed and authenticated
#   - cargo and Rust toolchain
#   - git with access to the repository

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
CARGO_TOML="$REPO_ROOT/crates/ralph/Cargo.toml"
CHANGELOG="$REPO_ROOT/CHANGELOG.md"
RELEASE_NOTES_TEMPLATE="$REPO_ROOT/.github/release-notes-template.md"
RELEASE_ARTIFACTS_DIR="$REPO_ROOT/target/release-artifacts"

# Dry run mode
DRY_RUN="${RELEASE_DRY_RUN:-0}"

# Version from argument
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

# Dry run aware command execution
run_cmd() {
    if [ "$DRY_RUN" = "1" ]; then
        echo "    [DRY RUN] Would execute: $*"
    else
        "$@"
    fi
}

# Validate semver format
validate_version() {
    local version="$1"
    if [[ ! "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
        log_error "VERSION must be in semver format (e.g., 0.2.0)"
        exit 1
    fi
}

# Check if required tools are installed
check_prerequisites() {
    log_step "Checking prerequisites"

    # Check gh CLI
    if ! command -v gh &> /dev/null; then
        log_error "GitHub CLI (gh) is not installed"
        echo "  Install from: https://cli.github.com/"
        exit 1
    fi
    log_success "GitHub CLI (gh) found"

    # Check gh authentication
    if ! gh auth status &> /dev/null; then
        log_error "GitHub CLI is not authenticated"
        echo "  Run: gh auth login"
        exit 1
    fi
    log_success "GitHub CLI authenticated"

    # Check cargo
    if ! command -v cargo &> /dev/null; then
        log_error "cargo is not installed"
        exit 1
    fi
    log_success "cargo found"

    # Check git
    if ! command -v git &> /dev/null; then
        log_error "git is not installed"
        exit 1
    fi
    log_success "git found"
}

# Pre-release validation
validate_repo_state() {
    log_step "Validating repository state"

    cd "$REPO_ROOT"

    # Check we're on main branch
    local current_branch
    current_branch=$(git branch --show-current)
    if [ "$current_branch" != "main" ]; then
        log_error "Not on main branch (currently on: $current_branch)"
        echo "  Releases must be created from the main branch"
        exit 1
    fi
    log_success "On main branch"

    # Check working directory is clean (excluding .ralph/* files)
    local dirty_files
    dirty_files=$(git status --porcelain | grep -v '^\.ralph/' || true)
    if [ -n "$dirty_files" ]; then
        log_error "Working directory is not clean"
        echo "  Dirty files:"
        echo "$dirty_files" | sed 's/^/    /'
        echo "  Commit or stash changes before releasing"
        exit 1
    fi
    log_success "Working directory is clean"

    # Check remote is accessible
    if ! git ls-remote &> /dev/null; then
        log_error "Cannot access git remote"
        echo "  Check your network connection and remote configuration"
        exit 1
    fi
    log_success "Git remote is accessible"

    # Check if tag already exists
    if git rev-parse "v$VERSION" &> /dev/null; then
        log_error "Tag v$VERSION already exists"
        echo "  Delete existing tag with: git tag -d v$VERSION"
        exit 1
    fi
    log_success "Tag v$VERSION does not exist"
}

# Update version in Cargo.toml
update_cargo_version() {
    log_step "Updating version in Cargo.toml"

    local current_version
    current_version=$(grep '^version = ' "$CARGO_TOML" | head -1 | sed 's/version = "\(.*\)"/\1/')
    log_info "Current version: $current_version"
    log_info "New version: $VERSION"

    if [ "$DRY_RUN" = "1" ]; then
        echo "    [DRY RUN] Would update $CARGO_TOML"
    else
        sed -i.bak -E "s/^version = \"[0-9]+\.[0-9]+\.[0-9]+\"/version = \"$VERSION\"/" "$CARGO_TOML"
        rm -f "$CARGO_TOML.bak"
        log_success "Updated version in Cargo.toml"
    fi
}

# Generate changelog entries from commits
generate_changelog_entries() {
    log_step "Generating changelog entries from commits"

    if [ "$DRY_RUN" = "1" ]; then
        echo "    [DRY RUN] Would run: scripts/generate-changelog.sh"
        echo "    [DRY RUN]   - Generate entries from RQ-#### commits since last tag"
        echo "    [DRY RUN]   - Update CHANGELOG.md Unreleased section"
    else
        if ! "$SCRIPT_DIR/generate-changelog.sh"; then
            log_warn "Changelog generation had issues, continuing with manual update"
        else
            log_success "Generated changelog entries"
        fi
    fi
}

# Update CHANGELOG.md with version section
update_changelog() {
    log_step "Updating CHANGELOG.md"

    local today
    today=$(date +%Y-%m-%d)

    if [ "$DRY_RUN" = "1" ]; then
        echo "    [DRY RUN] Would update $CHANGELOG"
        echo "    [DRY RUN]   - Move Unreleased content to version $VERSION section"
        echo "    [DRY RUN]   - Update comparison links"
    else
        # Move Unreleased content to new version section
        # This preserves the generated entries and creates a new empty Unreleased section

        # Create temp file for processing
        local temp_file
        temp_file=$(mktemp)

        # Read current changelog and transform it
        local in_unreleased=0
        local found_unreleased=0
        local unreleased_content=""
        local before_unreleased=""
        local after_unreleased=""

        while IFS= read -r line || [ -n "$line" ]; do
            if [ "$found_unreleased" -eq 0 ]; then
                # Looking for ## [Unreleased]
                if [[ "$line" =~ ^##\ \[Unreleased\] ]]; then
                    found_unreleased=1
                    in_unreleased=1
                    # Don't include the Unreleased header in before
                else
                    before_unreleased="$before_unreleased$line"$'\n'
                fi
            elif [ "$in_unreleased" -eq 1 ]; then
                # Inside Unreleased section, looking for next ##
                if [[ "$line" =~ ^##\ \[ ]]; then
                    in_unreleased=0
                    after_unreleased="$line"$'\n'
                else
                    unreleased_content="$unreleased_content$line"$'\n'
                fi
            else
                # After Unreleased section
                after_unreleased="$after_unreleased$line"$'\n'
            fi
        done < "$CHANGELOG"

        if [ "$found_unreleased" -eq 0 ]; then
            log_error "Could not find ## [Unreleased] section in CHANGELOG.md"
            rm -f "$temp_file"
            exit 1
        fi

        # Clean up unreleased content (remove leading/trailing blank lines)
        unreleased_content=$(echo "$unreleased_content" | sed -e '/./,$!d' -e :a -e '/^\n*$/{$d;N;};/\n$/ba')

        # Get current base version from comparison link
        local current_base
        current_base=$(echo "$before_unreleased" | grep '^\[Unreleased\]:' | sed 's/.*compare\/v\([0-9.]*\)\.\.\.HEAD.*/\1/' || echo "0.1.0")

        # Write new changelog
        {
            # Header and everything before Unreleased
            echo -n "$before_unreleased"

            # New empty Unreleased section
            echo "## [Unreleased]"
            echo ""

            # New version section with the content
            echo "## [$VERSION] - $today"
            echo ""

            # Add the content from Unreleased (only if there's actual content)
            if [ -n "$unreleased_content" ]; then
                echo "$unreleased_content"
                echo ""
            fi

            # Everything after the old Unreleased section
            echo -n "$after_unreleased"

            # Update comparison links at the end
            # First, update the Unreleased link to point to new version
            # Then add the new version link
        } > "$temp_file"

        # Update the comparison links
        sed -i.bak \
            -e "s|^\[Unreleased\]: .*|[Unreleased]: https://github.com/mitchfultz/ralph/compare/v$VERSION...HEAD|" \
            -e "/^\[$current_base\]: /a\\
[$VERSION]: https://github.com/mitchfultz/ralph/releases/tag/v$VERSION" \
            "$temp_file"

        rm -f "$temp_file.bak"

        # Replace original with updated
        mv "$temp_file" "$CHANGELOG"

        log_success "Updated CHANGELOG.md"
    fi
}

# Run CI validation
run_ci() {
    log_step "Running CI validation"

    if [ "$DRY_RUN" = "1" ]; then
        echo "    [DRY RUN] Would run: make ci"
    else
        cd "$REPO_ROOT"
        if ! make ci; then
            log_error "CI validation failed"
            echo "  Fix issues before releasing"
            exit 1
        fi
        log_success "CI validation passed"
    fi
}

# Build release artifacts for current platform
build_release_artifacts() {
    log_step "Building release artifacts"

    local target_triple
    target_triple=$(rustc --print host-tuple)

    log_info "Building for target: $target_triple"

    if [ "$DRY_RUN" = "1" ]; then
        echo "    [DRY RUN] Would build release binary with: cargo build --release -p ralph"
        echo "    [DRY RUN] Would create tarball in: $RELEASE_ARTIFACTS_DIR"
    else
        # Build release binary
        cd "$REPO_ROOT"
        cargo build --release -p ralph --quiet

        # Create artifacts directory
        mkdir -p "$RELEASE_ARTIFACTS_DIR"

        # Determine platform name
        local platform_name
        case "$target_triple" in
            x86_64-unknown-linux-gnu|x86_64-unknown-linux-musl)
                platform_name="linux-x64"
                ;;
            x86_64-apple-darwin)
                platform_name="macos-x64"
                ;;
            aarch64-apple-darwin)
                platform_name="macos-arm64"
                ;;
            *)
                platform_name="$target_triple"
                ;;
        esac

        # Create tarball
        local tarball_name="ralph-${VERSION}-${platform_name}.tar.gz"
        local binary_path="$REPO_ROOT/target/release/ralph"

        if [ -f "$binary_path" ]; then
            tar -czf "$RELEASE_ARTIFACTS_DIR/$tarball_name" -C "$REPO_ROOT/target/release" ralph
            log_success "Created $tarball_name"
        else
            log_error "Binary not found at $binary_path"
            exit 1
        fi

        # Generate checksum
        cd "$RELEASE_ARTIFACTS_DIR"
        shasum -a 256 "$tarball_name" > "$tarball_name.sha256"
        log_success "Generated SHA256 checksum"
    fi
}

# Generate release notes from template
generate_release_notes() {
    log_step "Generating release notes"

    local release_notes_file="$REPO_ROOT/target/release-notes-v$VERSION.md"

    if [ "$DRY_RUN" = "1" ]; then
        echo "    [DRY RUN] Would generate release notes from template"
    else
        # Extract changelog section for this version
        local changelog_section
        changelog_section=$(awk "/## \[$VERSION\] - /{flag=1;next}/## \[/{flag=0}flag" "$CHANGELOG" | sed '/^$/N;/^\n$/D')

        if [ -z "$changelog_section" ]; then
            changelog_section="See CHANGELOG.md for details."
        fi

        # Generate checksums section
        local checksums=""
        if [ -d "$RELEASE_ARTIFACTS_DIR" ]; then
            checksums=$(cd "$RELEASE_ARTIFACTS_DIR" && cat ./*.sha256 2>/dev/null || echo "Checksums not available")
        fi

        # Read template and substitute
        if [ -f "$RELEASE_NOTES_TEMPLATE" ]; then
            # Use template
            sed -e "s/{{VERSION}}/$VERSION/g" \
                -e "s/{{CHANGELOG_SECTION}}/$changelog_section/g" \
                -e "s/{{CHECKSUMS}}/$checksums/g" \
                "$RELEASE_NOTES_TEMPLATE" > "$release_notes_file"
        else
            # Generate simple release notes
            cat > "$release_notes_file" << EOF
## What's Changed

$changelog_section

## Installation

Download the appropriate binary for your platform, verify the checksum, then:

\`\`\`bash
tar -xzf ralph-${VERSION}-<platform>.tar.gz
mv ralph ~/.local/bin/
\`\`\`

## Checksums

\`\`\`
$checksums
\`\`\`
EOF
        fi

        log_success "Generated release notes: $release_notes_file"
        echo "$release_notes_file"
    fi
}

# Create git commit and tag
create_git_tag() {
    log_step "Creating git commit and tag"

    if [ "$DRY_RUN" = "1" ]; then
        echo "    [DRY RUN] Would stage: crates/ralph/Cargo.toml CHANGELOG.md"
        echo "    [DRY RUN] Would commit: Release v$VERSION"
        echo "    [DRY RUN] Would tag: v$VERSION (annotated)"
    else
        cd "$REPO_ROOT"

        # Stage changes
        git add crates/ralph/Cargo.toml CHANGELOG.md

        # Create commit
        git commit -m "Release v$VERSION"
        log_success "Created commit"

        # Create annotated tag
        git tag -a "v$VERSION" -m "Release v$VERSION"
        log_success "Created annotated tag v$VERSION"
    fi
}

# Create GitHub release
 create_github_release() {
    log_step "Creating GitHub release"

    local release_notes_file="$REPO_ROOT/target/release-notes-v$VERSION.md"

    if [ "$DRY_RUN" = "1" ]; then
        echo "    [DRY RUN] Would create GitHub release with:"
        echo "    [DRY RUN]   gh release create v$VERSION --verify-tag --notes-file $release_notes_file"
        if [ -d "$RELEASE_ARTIFACTS_DIR" ]; then
            for artifact in "$RELEASE_ARTIFACTS_DIR"/*.tar.gz; do
                if [ -f "$artifact" ]; then
                    echo "    [DRY RUN]   gh release upload v$VERSION $artifact"
                fi
            done
        fi
    else
        # Create release
        if ! gh release create "v$VERSION" \
            --verify-tag \
            --notes-file "$release_notes_file"; then
            log_error "Failed to create GitHub release"
            echo "  You may need to push the tag first: git push origin v$VERSION"
            exit 1
        fi
        log_success "Created GitHub release v$VERSION"

        # Upload artifacts
        if [ -d "$RELEASE_ARTIFACTS_DIR" ]; then
            for artifact in "$RELEASE_ARTIFACTS_DIR"/*.tar.gz; do
                if [ -f "$artifact" ]; then
                    local checksum_file="${artifact}.sha256"
                    if ! gh release upload "v$VERSION" "$artifact" "$checksum_file"; then
                        log_warn "Failed to upload $artifact"
                    else
                        log_success "Uploaded $(basename "$artifact")"
                    fi
                fi
            done
        fi
    fi
}

# Push to remote
push_to_remote() {
    log_step "Pushing to remote"

    if [ "$DRY_RUN" = "1" ]; then
        echo "    [DRY RUN] Would push main branch and tag"
        echo "    [DRY RUN]   git push origin main"
        echo "    [DRY RUN]   git push origin v$VERSION"
    else
        cd "$REPO_ROOT"

        log_info "Pushing main branch..."
        git push origin main

        log_info "Pushing tag v$VERSION..."
        git push origin "v$VERSION"

        log_success "Pushed to remote"
    fi
}

# Rollback function (for cleanup on failure)
rollback() {
    log_warn "Release failed. Rolling back changes..."

    cd "$REPO_ROOT"

    # Reset changes to Cargo.toml and CHANGELOG.md
    git checkout -- crates/ralph/Cargo.toml CHANGELOG.md 2>/dev/null || true

    # Delete local tag if created
    if git rev-parse "v$VERSION" &> /dev/null; then
        git tag -d "v$VERSION" 2>/dev/null || true
    fi

    # Clean up artifacts
    rm -rf "$RELEASE_ARTIFACTS_DIR"
    rm -f "$REPO_ROOT/target/release-notes-v$VERSION.md"

    log_info "Rollback complete"
}

# Print summary
print_summary() {
    echo ""
    echo "═══════════════════════════════════════════════════"
    if [ "$DRY_RUN" = "1" ]; then
        echo -e "  ${YELLOW}DRY RUN COMPLETE${NC}"
    else
        echo -e "  ${GREEN}RELEASE COMPLETE${NC}"
    fi
    echo "═══════════════════════════════════════════════════"
    echo "  Version: v$VERSION"
    echo ""

    if [ "$DRY_RUN" = "1" ]; then
        echo "  This was a dry run. No changes were made."
        echo "  To perform the actual release, run:"
        echo "    scripts/release.sh $VERSION"
    else
        echo "  Release v$VERSION has been created and published!"
        echo ""
        echo "  Next steps:"
        echo "    1. Verify the release on GitHub:"
        echo "       gh release view v$VERSION"
        echo "    2. Install the new version:"
        echo "       make install"
    fi
    echo "═══════════════════════════════════════════════════"
}

# Main function
main() {
    echo "═══════════════════════════════════════════════════"
    if [ "$DRY_RUN" = "1" ]; then
        echo -e "  ${YELLOW}RALPH RELEASE (DRY RUN)${NC}"
    else
        echo -e "  ${GREEN}RALPH RELEASE${NC}"
    fi
    echo "═══════════════════════════════════════════════════"
    echo ""

    # Validate arguments
    if [ -z "$VERSION" ]; then
        log_error "VERSION is required"
        echo "  Usage: scripts/release.sh <version>"
        echo "  Example: scripts/release.sh 0.2.0"
        echo ""
        echo "  Dry run mode: RELEASE_DRY_RUN=1 scripts/release.sh <version>"
        exit 1
    fi

    validate_version "$VERSION"

    if [ "$DRY_RUN" = "1" ]; then
        log_warn "DRY RUN MODE - No changes will be made"
        echo ""
    fi

    # Trap for cleanup on error
    if [ "$DRY_RUN" != "1" ]; then
        trap rollback ERR
    fi

    # Run release steps
    check_prerequisites
    validate_repo_state
    update_cargo_version
    generate_changelog_entries
    update_changelog
    run_ci
    build_release_artifacts
    generate_release_notes
    create_git_tag

    # Push and create GitHub release
    push_to_remote
    create_github_release

    # Disable trap on success
    if [ "$DRY_RUN" != "1" ]; then
        trap - ERR
    fi

    print_summary
}

main "$@"
