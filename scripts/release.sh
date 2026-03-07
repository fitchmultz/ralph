#!/usr/bin/env bash
#
# Purpose: Execute Ralph's local-only release workflow (no GitHub Actions).
# Responsibilities:
# - Validate release preconditions (branch, cleanliness, auth/tooling, tag availability).
# - Update release metadata (VERSION, Cargo version, app metadata, and CHANGELOG sections/links).
# - Run local CI, package artifacts, generate checksums, and publish via GitHub CLI.
# Scope:
# - Operates on repository metadata and release artifacts only.
# - Does not perform dependency upgrades or unrelated refactors.
# Usage:
# - scripts/release.sh <version>
# - RELEASE_DRY_RUN=1 scripts/release.sh <version>
# Invariants/assumptions:
# - Version must be strict semver (x.y.z).
# - Release flow runs from repository root on the main branch.
# - CI and release artifact steps must be reproducible (`--locked`).

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

VERSION_FILE="$REPO_ROOT/VERSION"
CARGO_TOML="$REPO_ROOT/crates/ralph/Cargo.toml"
CHANGELOG="$REPO_ROOT/CHANGELOG.md"
RELEASE_NOTES_TEMPLATE="$REPO_ROOT/.github/release-notes-template.md"
RELEASE_ARTIFACTS_DIR="$REPO_ROOT/target/release-artifacts"
CRATE_PACKAGE_NAME="ralph-agent-loop"
ALLOWED_RELEASE_DIRTY_PATHS=(
    "VERSION"
    "crates/ralph/Cargo.toml"
    "apps/RalphMac/RalphMac.xcodeproj/project.pbxproj"
    "apps/RalphMac/RalphCore/VersionValidator.swift"
    "CHANGELOG.md"
    "schemas/config.schema.json"
    "schemas/queue.schema.json"
)

# Show usage information
usage() {
    echo "Release script for Ralph - Local release workflow without GitHub Actions"
    echo ""
    echo "Usage:"
    echo "  scripts/release.sh <version>              # Full release"
    echo "  scripts/release.sh --help                 # Show this help"
    echo ""
    echo "Arguments:"
    echo "  <version>   Version number in semver format (e.g., 0.2.0, 1.0.0)"
    echo ""
    echo "Environment Variables:"
    echo "  RELEASE_DRY_RUN            Set to 1 for dry run mode (no side effects)"
    echo "  RALPH_RELEASE_SKIP_PUBLISH Set to 1 to skip crates.io publication"
    echo "  RALPH_MAKE_CMD             Override make executable for CI step (e.g., gmake)"
    echo ""
    echo "Examples:"
    echo "  # Full release"
    echo "  scripts/release.sh 0.2.0"
    echo ""
    echo "  # Dry run mode (preview without making changes)"
    echo "  RELEASE_DRY_RUN=1 scripts/release.sh 0.2.0"
    echo ""
    echo "  # Show this help"
    echo "  scripts/release.sh --help"
    echo "  scripts/release.sh -h"
    echo ""
    echo "Prerequisites:"
    echo "  - gh CLI installed and authenticated"
    echo "  - cargo and Rust toolchain"
    echo "  - git with access to the repository"
    echo ""
    echo "Exit codes:"
    echo "  0  Success"
    echo "  1  Runtime or unexpected failure"
    echo "  2  Usage/validation error"
    echo ""
    echo "Release Process:"
    echo "  1. Pre-release validation (clean working dir, main branch, CI passes)"
    echo "  2. Version metadata sync (VERSION, Cargo.toml, Xcode, app compatibility)"
    echo "  3. CHANGELOG.md updates"
    echo "  4. crates.io packaging review + publish dry-run"
    echo "  5. crates.io publication for $CRATE_PACKAGE_NAME (unless skipped)"
    echo "  6. Release artifact build for the current platform"
    echo "  7. Checksum generation (SHA256)"
    echo "  8. Git commit and annotated tag creation"
    echo "  9. GitHub release creation with asset upload via gh CLI"
    echo ""
    echo "Notes:"
    echo "  - If $CRATE_PACKAGE_NAME v<version> is already on crates.io, the script skips"
    echo "    crates.io publication and continues with the Git/GitHub release steps."
}

# Dry run mode
DRY_RUN="${RELEASE_DRY_RUN:-0}"
SKIP_PUBLISH="${RALPH_RELEASE_SKIP_PUBLISH:-0}"
CRATE_PUBLISHED=0
REPO_HTTP_URL=""

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

resolve_make_cmd() {
    if [ -n "${RALPH_MAKE_CMD:-}" ]; then
        echo "$RALPH_MAKE_CMD"
        return
    fi

    if command -v gmake >/dev/null 2>&1; then
        echo "gmake"
        return
    fi

    if command -v make >/dev/null 2>&1 && make --version 2>/dev/null | grep -q "GNU Make"; then
        echo "make"
        return
    fi

    echo "GNU Make is required to run the release CI step. Install with 'brew install make' and use gmake." >&2
    exit 1
}

get_rust_host_target() {
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

    log_error "Unable to determine rustc host target"
    exit 1
}

mktemp_file() {
    local prefix="$1"
    local base="${TMPDIR:-/tmp}"
    base="${base%/}"
    mktemp "${base}/${prefix}.XXXXXX"
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

get_repo_http_url() {
    local remote_url
    remote_url=$(git -C "$REPO_ROOT" remote get-url origin 2>/dev/null || true)
    if [ -z "$remote_url" ]; then
        log_error "Failed to resolve git remote 'origin' URL"
        exit 1
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
            log_error "Unsupported origin remote URL format: $remote_url"
            exit 1
            ;;
    esac
}

render_release_notes_template() {
    local template_path="$1"
    local output_path="$2"
    local version="$3"
    local changelog_path="$4"
    local checksums_path="$5"
    local repo_url="$6"

    python3 - "$template_path" "$output_path" "$version" "$changelog_path" "$checksums_path" "$repo_url" <<'PY'
from pathlib import Path
import sys

template_path, output_path, version, changelog_path, checksums_path, repo_url = sys.argv[1:7]
template = Path(template_path).read_text(encoding="utf-8")
changelog = Path(changelog_path).read_text(encoding="utf-8").rstrip("\n")
checksums = Path(checksums_path).read_text(encoding="utf-8").rstrip("\n")
rendered = (
    template.replace("{{VERSION}}", version)
    .replace("{{REPO_URL}}", repo_url)
    .replace("{{REPO_CLONE_URL}}", f"{repo_url}.git")
    .replace("{{CHANGELOG_SECTION}}", changelog)
    .replace("{{CHECKSUMS}}", checksums)
)
Path(output_path).write_text(rendered, encoding="utf-8")
PY
}

# Dry run aware command execution
run_cmd() {
    if [ "$DRY_RUN" = "1" ]; then
        echo "    [DRY RUN] Would execute: $*"
    else
        "$@"
    fi
}

is_allowed_release_dirty_path() {
    local path="$1"
    local allowed
    for allowed in "${ALLOWED_RELEASE_DIRTY_PATHS[@]}"; do
        if [ "$path" = "$allowed" ]; then
            return 0
        fi
    done
    return 1
}

assert_release_dirty_paths_allowed() {
    log_info "Validating post-CI dirty paths are release-expected"

    local line
    local path
    local disallowed=()
    local dirty_lines
    dirty_lines=$(git -C "$REPO_ROOT" status --porcelain | grep -vE '^..[[:space:]]+\.ralph/' || true)

    if [ -z "$dirty_lines" ]; then
        log_success "No tracked changes after CI"
        return 0
    fi

    while IFS= read -r line; do
        [ -z "$line" ] && continue
        path=$(echo "$line" | awk '{print $NF}')
        if ! is_allowed_release_dirty_path "$path"; then
            disallowed+=("$line")
        fi
    done <<< "$dirty_lines"

    if [ ${#disallowed[@]} -ne 0 ]; then
        log_error "CI introduced unexpected tracked changes"
        printf '  %s\n' "${disallowed[@]}"
        echo "  Allowed release-dirty paths are:"
        printf '    - %s\n' "${ALLOWED_RELEASE_DIRTY_PATHS[@]}"
        echo "  Resolve these changes before releasing."
        exit 1
    fi

    log_success "Post-CI tracked changes are release-expected"
}

ensure_release_binary() {
    local binary_path="$REPO_ROOT/target/release/ralph"

    if [ -x "$binary_path" ]; then
        log_info "Using existing release binary: $binary_path"
        return 0
    fi

    log_info "Release binary missing; building with locked dependencies"
    (
        cd "$REPO_ROOT"
        cargo build --release -p ralph-agent-loop --locked --quiet
    )
}

publish_crate() {
    log_step "Publishing crate to crates.io"

    if [ "$SKIP_PUBLISH" = "1" ]; then
        log_warn "Skipping crates.io publication because RALPH_RELEASE_SKIP_PUBLISH=1"
        return 0
    fi

    if [ "$DRY_RUN" = "1" ]; then
        echo "    [DRY RUN] Would review packaged files: cargo package --list -p $CRATE_PACKAGE_NAME"
        echo "    [DRY RUN] Would run publish dry-run: cargo publish --dry-run -p $CRATE_PACKAGE_NAME --locked --allow-dirty"
        echo "    [DRY RUN] Would publish crate: cargo publish -p $CRATE_PACKAGE_NAME --locked --allow-dirty"
        return 0
    fi

    cd "$REPO_ROOT"

    if curl -fsS "https://crates.io/api/v1/crates/$CRATE_PACKAGE_NAME/$VERSION" >/dev/null 2>&1; then
        log_warn "$CRATE_PACKAGE_NAME v$VERSION is already published on crates.io; skipping publication"
        return 0
    fi

    log_info "Reviewing packaged files for $CRATE_PACKAGE_NAME"
    cargo package --list -p "$CRATE_PACKAGE_NAME"

    log_info "Running crates.io publish dry-run"
    cargo publish --dry-run -p "$CRATE_PACKAGE_NAME" --locked --allow-dirty

    log_info "Publishing $CRATE_PACKAGE_NAME to crates.io"
    cargo publish -p "$CRATE_PACKAGE_NAME" --locked --allow-dirty
    CRATE_PUBLISHED=1

    log_success "Published $CRATE_PACKAGE_NAME to crates.io"
}

# Check if required tools are installed
check_prerequisites() {
    log_step "Checking prerequisites"

    local cargo_token_file="${CARGO_HOME:-$HOME/.cargo}/credentials.toml"

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

    # Check python3 for version metadata synchronization and release notes rendering
    if ! command -v python3 &> /dev/null; then
        log_error "python3 is not installed"
        exit 1
    fi
    log_success "python3 found"

    if [ "$SKIP_PUBLISH" != "1" ]; then
        if [ -n "${CARGO_REGISTRY_TOKEN:-}" ] || [ -f "$cargo_token_file" ]; then
            log_success "crates.io publish credentials found"
        else
            log_error "crates.io publish credentials not found"
            echo "  Run: cargo login"
            echo "  Or set CARGO_REGISTRY_TOKEN for this release"
            exit 1
        fi
    fi

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
    dirty_files=$(git status --porcelain | grep -vE '^..[[:space:]]+\.ralph/' || true)
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

# Update canonical and derived version metadata.
sync_release_version_metadata() {
    log_step "Syncing version metadata"

    local current_version
    current_version=$(read_canonical_version)
    log_info "Current version: $current_version"
    log_info "New version: $VERSION"

    if [ "$DRY_RUN" = "1" ]; then
        echo "    [DRY RUN] Would update $VERSION_FILE"
        echo "    [DRY RUN] Would sync $CARGO_TOML"
        echo "    [DRY RUN] Would sync apps/RalphMac/RalphMac.xcodeproj/project.pbxproj"
        echo "    [DRY RUN] Would sync apps/RalphMac/RalphCore/VersionValidator.swift"
    else
        sync_version_metadata "$VERSION"
        log_success "Version metadata synchronized"
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

        # Validate changelog link assumptions even in dry-run mode.
        local dry_run_base_version
        dry_run_base_version=$(sed -n -E 's|^\[Unreleased\]: .*compare/v([0-9]+\.[0-9]+\.[0-9]+)\.\.\.HEAD.*|\1|p' "$CHANGELOG" | head -1 || true)
        if [ -z "$dry_run_base_version" ]; then
            dry_run_base_version=$(sed -n -E 's|^## \[([0-9]+\.[0-9]+\.[0-9]+)\].*|\1|p' "$CHANGELOG" | head -1 || true)
        fi
        if [ -z "$dry_run_base_version" ]; then
            log_error "Dry-run validation failed: could not determine previous release version from CHANGELOG.md"
            exit 1
        fi
        log_success "Dry-run validated changelog base release version: $dry_run_base_version"
    else
        # Move Unreleased content to new version section
        # This preserves the generated entries and creates a new empty Unreleased section

        # Create temp file for processing
        local temp_file
        temp_file=$(mktemp_file "ralph-release")

        # Resolve current base version from existing Unreleased compare link.
        local unreleased_base_version
        unreleased_base_version=$(sed -n -E 's|^\[Unreleased\]: .*compare/v([0-9]+\.[0-9]+\.[0-9]+)\.\.\.HEAD.*|\1|p' "$CHANGELOG" | head -1 || true)
        if [ -z "$unreleased_base_version" ]; then
            unreleased_base_version=$(sed -n -E 's|^## \[([0-9]+\.[0-9]+\.[0-9]+)\].*|\1|p' "$CHANGELOG" | head -1 || true)
        fi
        if [ -z "$unreleased_base_version" ]; then
            log_error "Could not determine previous release version from CHANGELOG.md"
            rm -f "$temp_file"
            exit 1
        fi

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

        # Update comparison links (Keep a Changelog style):
        # - Unreleased now compares from new release tag to HEAD
        # - New release compares previous base release to new release
        if grep -q '^\[Unreleased\]:' "$temp_file"; then
            sed -i.bak \
                -e "/^\[$VERSION\]: /d" \
                -e "s|^\[Unreleased\]: .*|[Unreleased]: $REPO_HTTP_URL/compare/v$VERSION...HEAD|" \
                -e "/^\[Unreleased\]: /a\\
[$VERSION]: $REPO_HTTP_URL/compare/v$unreleased_base_version...v$VERSION" \
                "$temp_file"
            rm -f "$temp_file.bak"
        else
            {
                echo ""
                echo "[Unreleased]: $REPO_HTTP_URL/compare/v$VERSION...HEAD"
                echo "[$VERSION]: $REPO_HTTP_URL/compare/v$unreleased_base_version...v$VERSION"
            } >> "$temp_file"
        fi

        # Replace original with updated
        mv "$temp_file" "$CHANGELOG"

        log_success "Updated CHANGELOG.md"
    fi
}

# Run CI validation
run_ci() {
    log_step "Running CI validation"

    if [ "$DRY_RUN" = "1" ]; then
        local make_cmd
        make_cmd=$(resolve_make_cmd)
        echo "    [DRY RUN] Would run: ${make_cmd} ci"
        echo "    [DRY RUN] Would validate dirty paths after CI are release-expected"
    else
        local make_cmd
        make_cmd=$(resolve_make_cmd)
        cd "$REPO_ROOT"
        if ! "$make_cmd" ci; then
            log_error "CI validation failed"
            echo "  Fix issues before releasing"
            exit 1
        fi
        log_success "CI validation passed"
        assert_release_dirty_paths_allowed
    fi
}

# Build release artifacts for current platform
build_release_artifacts() {
    log_step "Building release artifacts"

    local target_triple
    target_triple=$(get_rust_host_target)

    log_info "Building for target: $target_triple"

    if [ "$DRY_RUN" = "1" ]; then
        echo "    [DRY RUN] Would ensure release binary exists (locked build if needed)"
        echo "    [DRY RUN] Would create tarball in: $RELEASE_ARTIFACTS_DIR"
    else
        ensure_release_binary

        # Start from a clean artifact directory so stale tarballs/checksums from
        # earlier releases cannot be uploaded with the current release.
        rm -rf "$RELEASE_ARTIFACTS_DIR"
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
        sha256_file "$tarball_name" > "$tarball_name.sha256"
        log_success "Generated SHA256 checksum"
    fi
}

# Generate release notes from template
generate_release_notes() {
    log_step "Generating release notes"

    local release_notes_file="$REPO_ROOT/target/release-notes-v$VERSION.md"

    if [ "$DRY_RUN" = "1" ]; then
        echo "    [DRY RUN] Would generate release notes from template"

        if [ -f "$RELEASE_NOTES_TEMPLATE" ] && command -v python3 >/dev/null 2>&1; then
            local preview_file
            local preview_changelog
            local preview_checksums
            preview_file=$(mktemp_file "ralph-release-notes-preview")
            preview_changelog=$(mktemp_file "ralph-release-notes-preview-changelog")
            preview_checksums=$(mktemp_file "ralph-release-notes-preview-checksums")
            printf 'Dry-run preview changelog section\n' > "$preview_changelog"
            printf 'ralph-%s-sample.tar.gz  abcdef\n' "$VERSION" > "$preview_checksums"

            render_release_notes_template \
                "$RELEASE_NOTES_TEMPLATE" \
                "$preview_file" \
                "$VERSION" \
                "$preview_changelog" \
                "$preview_checksums" \
                "$REPO_HTTP_URL"

            if ! grep -q "$VERSION" "$preview_file"; then
                log_error "Dry-run validation failed: rendered release notes missing version marker"
                rm -f "$preview_file" "$preview_changelog" "$preview_checksums"
                exit 1
            fi

            rm -f "$preview_file" "$preview_changelog" "$preview_checksums"
            log_success "Dry-run validated release-notes template rendering"
        elif [ -f "$RELEASE_NOTES_TEMPLATE" ]; then
            log_warn "Dry-run could not validate template rendering because python3 is unavailable"
        fi
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

        # Render release notes.
        local changelog_tmp
        local checksums_tmp
        changelog_tmp=$(mktemp_file "ralph-release-notes-changelog")
        checksums_tmp=$(mktemp_file "ralph-release-notes-checksums")
        printf '%s\n' "$changelog_section" > "$changelog_tmp"
        printf '%s\n' "$checksums" > "$checksums_tmp"

        render_fallback_release_notes() {
            cat > "$release_notes_file" << EOF_RELEASE_NOTES
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
EOF_RELEASE_NOTES
        }

        if [ -f "$RELEASE_NOTES_TEMPLATE" ] && command -v python3 >/dev/null 2>&1; then
            render_release_notes_template \
                "$RELEASE_NOTES_TEMPLATE" \
                "$release_notes_file" \
                "$VERSION" \
                "$changelog_tmp" \
                "$checksums_tmp" \
                "$REPO_HTTP_URL"
        elif [ -f "$RELEASE_NOTES_TEMPLATE" ]; then
            log_warn "python3 not found; using fallback release notes format"
            render_fallback_release_notes
        else
            render_fallback_release_notes
        fi

        rm -f "$changelog_tmp" "$checksums_tmp"

        log_success "Generated release notes: $release_notes_file"
        echo "$release_notes_file"
    fi
}

# Create git commit and tag
create_git_tag() {
    log_step "Creating git commit and tag"

    if [ "$DRY_RUN" = "1" ]; then
        echo "    [DRY RUN] Would stage: VERSION crates/ralph/Cargo.toml apps/RalphMac/RalphMac.xcodeproj/project.pbxproj apps/RalphMac/RalphCore/VersionValidator.swift CHANGELOG.md schemas/config.schema.json schemas/queue.schema.json"
        echo "    [DRY RUN] Would commit: Release v$VERSION"
        echo "    [DRY RUN] Would tag: v$VERSION (annotated)"
    else
        cd "$REPO_ROOT"

        # Stage release metadata + generated schemas (if changed by CI/generate)
        git add VERSION crates/ralph/Cargo.toml apps/RalphMac/RalphMac.xcodeproj/project.pbxproj apps/RalphMac/RalphCore/VersionValidator.swift CHANGELOG.md schemas/config.schema.json schemas/queue.schema.json

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
        echo "    [DRY RUN]   gh release create v$VERSION --title v$VERSION --verify-tag --notes-file $release_notes_file"
        if [ -d "$RELEASE_ARTIFACTS_DIR" ]; then
            for artifact in "$RELEASE_ARTIFACTS_DIR"/ralph-"${VERSION}"-*.tar.gz; do
                if [ -f "$artifact" ]; then
                    echo "    [DRY RUN]   gh release upload v$VERSION $artifact"
                fi
            done
        fi
    else
        # Create release
        if ! gh release create "v$VERSION" \
            --title "v$VERSION" \
            --verify-tag \
            --notes-file "$release_notes_file"; then
            log_error "Failed to create GitHub release"
            echo "  You may need to push the tag first: git push origin v$VERSION"
            exit 1
        fi
        log_success "Created GitHub release v$VERSION"

        # Upload artifacts
        if [ -d "$RELEASE_ARTIFACTS_DIR" ]; then
            for artifact in "$RELEASE_ARTIFACTS_DIR"/ralph-"${VERSION}"-*.tar.gz; do
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

    # Reset release metadata changes
    git checkout -- VERSION crates/ralph/Cargo.toml apps/RalphMac/RalphMac.xcodeproj/project.pbxproj apps/RalphMac/RalphCore/VersionValidator.swift CHANGELOG.md schemas/config.schema.json schemas/queue.schema.json 2>/dev/null || true

    # Delete local tag if created
    if git rev-parse "v$VERSION" &> /dev/null; then
        git tag -d "v$VERSION" 2>/dev/null || true
    fi

    # Clean up artifacts
    rm -rf "$RELEASE_ARTIFACTS_DIR"
    rm -f "$REPO_ROOT/target/release-notes-v$VERSION.md"

    if [ "$CRATE_PUBLISHED" = "1" ]; then
        log_warn "crates.io publication already succeeded for $CRATE_PACKAGE_NAME v$VERSION"
        echo "  The registry version is immutable and was not rolled back."
        echo "  Fix the failure, then rerun with:"
        echo "    RALPH_RELEASE_SKIP_PUBLISH=1 scripts/release.sh $VERSION"
    fi

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
        echo "    1. Verify the crate on crates.io:"
        echo "       cargo install $CRATE_PACKAGE_NAME"
        echo "    2. Verify the release on GitHub:"
        echo "       gh release view v$VERSION"
        echo "    3. Install the new version locally:"
        echo "       make install"
    fi
    echo "═══════════════════════════════════════════════════"
}

# Main function
main() {
    # Handle --help/-h before processing VERSION
    if [ "${1:-}" = "--help" ] || [ "${1:-}" = "-h" ]; then
        usage
        exit 0
    fi

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
        echo ""
        usage
        exit 2
    fi

    validate_semver "$VERSION" || {
        log_error "VERSION must be in semver format (e.g., 0.2.0)"
        exit 2
    }

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
    REPO_HTTP_URL=$(get_repo_http_url)
    validate_repo_state
    sync_release_version_metadata
    generate_changelog_entries
    update_changelog
    run_ci
    publish_crate
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
