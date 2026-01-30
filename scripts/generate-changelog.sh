#!/usr/bin/env bash
#
# Generate changelog entries from RQ-#### conventional commits.
#
# RESPONSIBILITY:
#   This script populates the CHANGELOG.md "Unreleased" section with
#   entries parsed from commits following the "RQ-####: <summary>" pattern.
#   It categorizes commits into Added/Changed/Fixed/Removed/Security sections
#   based on the commit message prefix.
#
# EXPLICITLY DOES NOT HANDLE:
#   - Version bumping or tagging (handled by release.sh)
#   - Moving Unreleased content to versioned sections (handled by release.sh)
#   - Non-RQ-#### commits (these are ignored by git-cliff configuration)
#   - Merge commits or commit bodies (only parses commit subject lines)
#
# CALLER INVARIANTS:
#   - Must have git-cliff installed (cargo install git-cliff)
#   - Must have cliff.toml configuration in repo root
#   - Must have CHANGELOG.md with an "## [Unreleased]" section
#   - Must be run from within the git repository
#
# Usage:
#   scripts/generate-changelog.sh           # Update CHANGELOG.md in place
#   scripts/generate-changelog.sh --dry-run # Preview changes without modifying files
#   scripts/generate-changelog.sh --check   # Check if changelog is up to date (CI)
#
# The script preserves manual edits in CHANGELOG.md by:
# 1. Generating new entries from commits since the last tag
# 2. Inserting them into the Unreleased section
# 3. Preserving existing content above and below

set -euo pipefail

# Script configuration
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
CHANGELOG="$REPO_ROOT/CHANGELOG.md"
CLIFF_CONFIG="$REPO_ROOT/cliff.toml"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Mode flags
DRY_RUN=0
CHECK_MODE=0

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

# Check if git-cliff is installed
check_git_cliff() {
    if ! command -v git-cliff &> /dev/null; then
        log_error "git-cliff is not installed"
        echo ""
        echo "Install with:"
        echo "  cargo install git-cliff"
        echo ""
        echo "Or visit: https://git-cliff.org/docs/installation"
        exit 1
    fi
}

# Get the latest tag
get_latest_tag() {
    git -C "$REPO_ROOT" describe --tags --abbrev=0 2>/dev/null || echo ""
}

# Generate changelog entries from commits
generate_entries() {
    local since_tag="${1:-}"

    # Generate full changelog and extract just the Unreleased section content
    local full_changelog
    if [ -n "$since_tag" ]; then
        full_changelog=$(git-cliff --config "$CLIFF_CONFIG" --unreleased 2>/dev/null || echo "")
    else
        full_changelog=$(git-cliff --config "$CLIFF_CONFIG" 2>/dev/null || echo "")
    fi

    # Extract just the content after "## [Unreleased]" header up to the next "## ["
    # This removes the header/footer and gives us just the entries
    echo "$full_changelog" | awk '/^## \[Unreleased\]/{found=1; next} /^## \[/{found=0} found'
}

# Update CHANGELOG.md with new entries
update_changelog() {
    local entries="$1"

    if [ -z "$entries" ]; then
        log_warn "No new entries to add"
        return 0
    fi

    # Check if entries has actual content (not just empty sections)
    local has_content=0
    while IFS= read -r line; do
        # Check for actual entry lines (start with "- ")
        if [[ "$line" =~ ^-[[:space:]] ]]; then
            has_content=1
            break
        fi
    done <<< "$entries"

    if [ "$has_content" -eq 0 ]; then
        log_warn "No new entries to add"
        return 0
    fi

    # Create a temporary file
    local temp_file
    temp_file=$(mktemp)

    # Clean up the entries - remove leading/trailing blank lines but preserve structure
    local cleaned_entries
    cleaned_entries=$(echo "$entries" | sed -e '/./,$!d' | sed -e :a -e '/^\n*$/{$d;N;};/\n$/ba')

    # Read the current changelog and update it
    local in_unreleased=0
    local found_unreleased=0
    local before_unreleased=""
    local after_unreleased=""

    while IFS= read -r line || [ -n "$line" ]; do
        if [ "$found_unreleased" -eq 0 ]; then
            # Looking for ## [Unreleased]
            if [[ "$line" =~ ^##\ \[Unreleased\] ]]; then
                found_unreleased=1
                in_unreleased=1
                before_unreleased="$before_unreleased$line"$'\n'
            else
                before_unreleased="$before_unreleased$line"$'\n'
            fi
        elif [ "$in_unreleased" -eq 1 ]; then
            # Inside Unreleased section, looking for next ##
            if [[ "$line" =~ ^##\ \[ ]]; then
                in_unreleased=0
                after_unreleased="$line"$'\n'
            fi
            # Skip existing content in Unreleased section - we'll replace it
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

    # Write the new changelog
    {
        echo -n "$before_unreleased"
        echo "$cleaned_entries"
        echo ""
        echo -n "$after_unreleased"
    } > "$temp_file"

    # Replace original with updated
    mv "$temp_file" "$CHANGELOG"

    log_success "Updated CHANGELOG.md"
}

# Preview changes without modifying files
preview_changes() {
    local since_tag
    since_tag=$(get_latest_tag)

    log_info "Previewing changelog entries since: ${since_tag:-'(beginning of history)'}"
    echo ""

    local entries
    entries=$(generate_entries "$since_tag")

    # Check if entries has actual content (not just empty sections)
    local has_content=0
    while IFS= read -r line; do
        # Check for actual entry lines (start with "- ")
        if [[ "$line" =~ ^-[[:space:]] ]]; then
            has_content=1
            break
        fi
    done <<< "$entries"

    if [ -z "$entries" ] || [ "$has_content" -eq 0 ]; then
        echo "No new entries would be added."
        echo ""
        echo "Make sure you have RQ-#### commits that match the patterns in cliff.toml"
    else
        echo "## [Unreleased]"
        echo "$entries"
    fi
}

# Check if changelog is up to date (for CI)
check_changelog() {
    local since_tag
    since_tag=$(get_latest_tag)

    local entries
    entries=$(generate_entries "$since_tag")

    # Check if there are any new entries
    if [ -z "$entries" ] || [ "$entries" = "## [Unreleased]" ]; then
        log_success "Changelog is up to date (no new entries to add)"
        exit 0
    fi

    # Check if entries are already in CHANGELOG.md
    local entry_count=0
    local missing_count=0

    # Extract commit messages from entries and check if they're in CHANGELOG
    while IFS= read -r line; do
        # Skip section headers and empty lines
        if [[ "$line" =~ ^### ]] || [[ -z "$line" ]]; then
            continue
        fi
        # Count entries (lines starting with "- ")
        if [[ "$line" =~ ^-\  ]]; then
            entry_count=$((entry_count + 1))
            # Check if this entry exists in CHANGELOG
            local message
            message=$(echo "$line" | sed 's/^- //')
            if ! grep -qF "$message" "$CHANGELOG" 2>/dev/null; then
                missing_count=$((missing_count + 1))
            fi
        fi
    done <<< "$entries"

    if [ "$missing_count" -eq 0 ]; then
        log_success "Changelog is up to date"
        exit 0
    else
        log_error "Changelog is out of date ($missing_count new entries missing)"
        echo ""
        echo "Run 'make changelog' to update CHANGELOG.md"
        exit 1
    fi
}

# Main update function
run_update() {
    local since_tag
    since_tag=$(get_latest_tag)

    log_info "Generating changelog entries since: ${since_tag:-'(beginning of history)'}"

    local entries
    entries=$(generate_entries "$since_tag")

    if [ "$DRY_RUN" -eq 1 ]; then
        echo ""
        echo "=== Would add the following entries ==="
        echo "$entries"
        echo "======================================="
    else
        update_changelog "$entries"
    fi
}

# Print usage
print_usage() {
    cat << 'EOF'
Usage: scripts/generate-changelog.sh [OPTIONS]

Generate changelog entries from RQ-#### conventional commits.

Options:
  --dry-run    Preview changes without modifying CHANGELOG.md
  --check      Check if changelog is up to date (exits with error if not)
  --help       Show this help message

Examples:
  scripts/generate-changelog.sh           # Update CHANGELOG.md
  scripts/generate-changelog.sh --dry-run # Preview changes
  make changelog                          # Same as above via Makefile
  make changelog-preview                  # Preview via Makefile

Requirements:
  - git-cliff installed (cargo install git-cliff)
  - cliff.toml configuration in repo root

Commit Message Patterns:
  RQ-####: Add ...       -> Added section
  RQ-####: Fix ...       -> Fixed section
  RQ-####: Update ...    -> Changed section
  RQ-####: Refactor ...  -> Changed section
  RQ-####: Remove ...    -> Removed section
  RQ-####: Security ...  -> Security section
EOF
}

# Main function
main() {
    # Parse arguments
    while [ $# -gt 0 ]; do
        case "$1" in
            --dry-run)
                DRY_RUN=1
                shift
                ;;
            --check)
                CHECK_MODE=1
                shift
                ;;
            --help|-h)
                print_usage
                exit 0
                ;;
            *)
                log_error "Unknown option: $1"
                print_usage
                exit 1
                ;;
        esac
    done

    # Check prerequisites
    check_git_cliff

    # Check if cliff.toml exists
    if [ ! -f "$CLIFF_CONFIG" ]; then
        log_error "Configuration file not found: $CLIFF_CONFIG"
        exit 1
    fi

    # Check if CHANGELOG.md exists
    if [ ! -f "$CHANGELOG" ]; then
        log_error "CHANGELOG.md not found at: $CHANGELOG"
        exit 1
    fi

    # Run in appropriate mode
    if [ "$CHECK_MODE" -eq 1 ]; then
        check_changelog
    elif [ "$DRY_RUN" -eq 1 ]; then
        preview_changes
    else
        run_update
    fi
}

main "$@"
