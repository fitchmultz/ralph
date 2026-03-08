#!/usr/bin/env bash
#
# Purpose: Implement the shared release transaction pipeline for Ralph.
# Responsibilities:
# - Validate prerequisites and repository state for release transactions.
# - Prepare and validate publish-ready local release snapshots.
# - Finalize git state only after a verified snapshot is accepted for publish.
# - Finalize remote publication to crates.io and GitHub in reconcile-safe phases.
# Scope:
# - Release pipeline orchestration helpers only; CLI parsing lives in scripts/release.sh.
# Usage:
# - source "$(dirname "$0")/lib/release_pipeline.sh"
# Invariants/assumptions:
# - Caller sets VERSION, REPO_ROOT, and release paths before invoking functions.
# - Verify flows initialize verification state before recording a publish-ready snapshot.
# - Execute/reconcile flows initialize transaction state before remote publication.

if [ -n "${RALPH_RELEASE_PIPELINE_SOURCED:-}" ]; then
    return 0
fi
RALPH_RELEASE_PIPELINE_SOURCED=1

release_check_prerequisites() {
    local require_publish_credentials="${1:-1}"

    ralph_log_step "Checking release prerequisites"

    local tool
    for tool in git cargo gh python3; do
        if ! command -v "$tool" >/dev/null 2>&1; then
            ralph_log_error "Required tool not found: $tool"
            return 1
        fi
        ralph_log_success "$tool found"
    done

    if ! gh auth status >/dev/null 2>&1; then
        ralph_log_error "GitHub CLI is not authenticated"
        echo "  Run: gh auth login" >&2
        return 1
    fi
    ralph_log_success "GitHub CLI authenticated"

    if [ "$require_publish_credentials" = "1" ]; then
        local cargo_token_file="${CARGO_HOME:-$HOME/.cargo}/credentials.toml"
        if [ -z "${CARGO_REGISTRY_TOKEN:-}" ] && [ ! -f "$cargo_token_file" ]; then
            ralph_log_error "crates.io publish credentials not found"
            echo "  Run: cargo login" >&2
            echo "  Or set CARGO_REGISTRY_TOKEN for this release" >&2
            return 1
        fi
        ralph_log_success "crates.io publish credentials found"
    fi
}

release_validate_repo_state() {
    local allow_existing_tag="${1:-0}"
    local allow_release_metadata_drift="${2:-0}"

    ralph_log_step "Validating repository state"
    cd "$REPO_ROOT"

    local current_branch
    current_branch=$(git branch --show-current)
    if [ "$current_branch" != "main" ]; then
        ralph_log_error "Not on main branch (currently on: $current_branch)"
        return 1
    fi
    ralph_log_success "On main branch"

    local dirty_files
    dirty_files=$(git status --porcelain | grep -vE '^..[[:space:]]+\.ralph/' || true)
    if [ -n "$dirty_files" ]; then
        if [ "$allow_release_metadata_drift" = "1" ] && release_assert_dirty_paths_allowed "$dirty_files"; then
            ralph_log_success "Working directory contains only release metadata drift"
        else
            ralph_log_error "Working directory is not clean"
            echo "$dirty_files" | sed 's/^/  /' >&2
            return 1
        fi
    else
        ralph_log_success "Working directory is clean"
    fi

    if ! git ls-remote origin >/dev/null 2>&1; then
        ralph_log_error "Cannot access git remote"
        return 1
    fi
    ralph_log_success "Git remote is accessible"

    if git rev-parse "v$VERSION" >/dev/null 2>&1; then
        if [ "$allow_existing_tag" = "1" ]; then
            ralph_log_warn "Local tag v$VERSION already exists; verify mode allows that"
        else
            ralph_log_error "Local tag v$VERSION already exists"
            echo "  Continue the recorded transaction with: scripts/release.sh reconcile $VERSION" >&2
            return 1
        fi
    else
        ralph_log_success "Local tag v$VERSION does not exist"
    fi

    if git ls-remote --tags origin "refs/tags/v$VERSION" | grep -q "refs/tags/v$VERSION"; then
        if [ "$allow_existing_tag" = "1" ]; then
            ralph_log_warn "Remote tag v$VERSION already exists; verify mode allows that"
        else
            ralph_log_error "Remote tag v$VERSION already exists"
            return 1
        fi
    else
        ralph_log_success "Remote tag v$VERSION does not exist"
    fi
}

release_run_ship_gate() {
    local make_cmd
    make_cmd=$(ralph_resolve_make_cmd)

    ralph_log_step "Running ship gate"
    cd "$REPO_ROOT"

    if [ "$(uname -s)" = "Darwin" ] && command -v xcodebuild >/dev/null 2>&1; then
        ralph_log_info "Running macOS ship gate"
        "$make_cmd" macos-ci
    else
        ralph_log_info "Running Rust ship gate"
        "$make_cmd" ci
    fi

    local dirty_lines
    dirty_lines=$(git status --porcelain | grep -vE '^..[[:space:]]+\.ralph/' || true)
    if ! release_assert_dirty_paths_allowed "$dirty_lines"; then
        return 1
    fi

    ralph_log_success "Ship gate passed"
}

release_generate_changelog_entries() {
    ralph_log_step "Generating changelog entries"
    cd "$REPO_ROOT"
    ./scripts/generate-changelog.sh
}

release_prepare_verified_snapshot() {
    ralph_log_step "Preparing verified release snapshot"

    cd "$REPO_ROOT"
    REPO_HTTP_URL=$(ralph_get_repo_http_url)
    ./scripts/versioning.sh sync --version "$VERSION"
    ./scripts/versioning.sh check
    release_generate_changelog_entries
    release_promote_changelog "$CHANGELOG" "$VERSION" "$(date +%Y-%m-%d)"
    ./scripts/pre-public-check.sh --skip-ci --release-context
    release_run_ship_gate
    ./scripts/build-release-artifacts.sh "$VERSION"
    release_generate_release_notes
    release_verify_record_ready_snapshot
    ralph_log_success "Verified release snapshot recorded at $VERIFY_DIR"
}

release_generate_release_notes() {
    ralph_log_step "Generating release notes"

    local changelog_section
    changelog_section=$(awk "/## \[$VERSION\] - /{flag=1;next}/## \[/{flag=0}flag" "$CHANGELOG" | sed '/^$/N;/^\n$/D')
    if [ -z "$changelog_section" ]; then
        changelog_section="See CHANGELOG.md for details."
    fi

    local changelog_tmp
    local checksums_tmp
    changelog_tmp=$(ralph_mktemp_file "ralph-release-notes-changelog")
    checksums_tmp=$(ralph_mktemp_file "ralph-release-notes-checksums")
    printf '%s\n' "$changelog_section" > "$changelog_tmp"
    if [ -d "$RELEASE_ARTIFACTS_DIR" ]; then
        (cd "$RELEASE_ARTIFACTS_DIR" && cat ./*.sha256 2>/dev/null || true) > "$checksums_tmp"
    else
        printf 'Checksums not available\n' > "$checksums_tmp"
    fi

    release_render_notes_template \
        "$RELEASE_NOTES_TEMPLATE" \
        "$RELEASE_NOTES_FILE" \
        "$VERSION" \
        "$changelog_tmp" \
        "$checksums_tmp" \
        "$REPO_HTTP_URL"

    rm -f "$changelog_tmp" "$checksums_tmp"
    ralph_log_success "Generated release notes: $RELEASE_NOTES_FILE"
}

release_create_commit_and_tag() {
    ralph_log_step "Creating release commit and tag"
    cd "$REPO_ROOT"

    git add "${RELEASE_METADATA_PATHS[@]}"
    git commit -m "Release v$VERSION"
    RELEASE_COMMIT=$(git rev-parse HEAD)
    git tag -a "v$VERSION" -m "Release v$VERSION"
    LOCAL_TAG_CREATED=1
    RELEASE_STATUS="prepared"
    release_state_write
    ralph_log_success "Created release commit $RELEASE_COMMIT and tag v$VERSION"
}

release_publish_crate() {
    if [ "$CRATE_PUBLISHED" = "1" ]; then
        ralph_log_info "crates.io publication already recorded for v$VERSION"
        return 0
    fi

    ralph_log_step "Publishing crate to crates.io"
    cd "$REPO_ROOT"
    cargo package --list -p "$CRATE_PACKAGE_NAME"
    cargo publish --dry-run -p "$CRATE_PACKAGE_NAME" --locked
    cargo publish -p "$CRATE_PACKAGE_NAME" --locked
    CRATE_PUBLISHED=1
    RELEASE_STATUS="crate_published"
    release_state_write
    ralph_log_success "Published $CRATE_PACKAGE_NAME v$VERSION"
}

release_push_remote_state() {
    if [ "$REMOTE_PUSHED" = "1" ]; then
        ralph_log_info "Remote push already recorded for v$VERSION"
        return 0
    fi

    ralph_log_step "Pushing release commit and tag"
    cd "$REPO_ROOT"
    git push origin main
    git push origin "v$VERSION"
    REMOTE_PUSHED=1
    RELEASE_STATUS="pushed"
    release_state_write
    ralph_log_success "Pushed main and v$VERSION"
}

release_create_github_release() {
    if [ "$GITHUB_RELEASE_CREATED" = "1" ]; then
        ralph_log_info "GitHub release already recorded for v$VERSION"
        return 0
    fi

    ralph_log_step "Creating GitHub release"
    gh release create "v$VERSION" \
        --title "v$VERSION" \
        --verify-tag \
        --notes-file "$RELEASE_NOTES_FILE"

    local artifact
    for artifact in "$RELEASE_ARTIFACTS_DIR"/ralph-"${VERSION}"-*.tar.gz; do
        [ -f "$artifact" ] || continue
        gh release upload "v$VERSION" "$artifact" "${artifact}.sha256"
    done

    GITHUB_RELEASE_CREATED=1
    RELEASE_STATUS="completed"
    release_state_write
    ralph_log_success "GitHub release v$VERSION created"
}

release_verify_plan() {
    ralph_log_step "Verifying release transaction contract"
    if ! release_validate_changelog_shape "$CHANGELOG"; then
        ralph_log_error "CHANGELOG.md is missing a release-compatible Unreleased section"
        return 1
    fi

    local preview_file
    local preview_changelog
    local preview_checksums
    preview_file=$(ralph_mktemp_file "ralph-release-preview")
    preview_changelog=$(ralph_mktemp_file "ralph-release-preview-changelog")
    preview_checksums=$(ralph_mktemp_file "ralph-release-preview-checksums")
    printf 'Preview changelog entry\n' > "$preview_changelog"
    printf 'ralph-%s-sample.tar.gz  abcdef\n' "$VERSION" > "$preview_checksums"

    local preview_repo_url
    preview_repo_url=$(ralph_get_repo_http_url)
    release_render_notes_template \
        "$RELEASE_NOTES_TEMPLATE" \
        "$preview_file" \
        "$VERSION" \
        "$preview_changelog" \
        "$preview_checksums" \
        "$preview_repo_url"

    if ! grep -q "$VERSION" "$preview_file"; then
        ralph_log_error "Rendered release notes preview is missing the version marker"
        rm -f "$preview_file" "$preview_changelog" "$preview_checksums"
        return 1
    fi

    rm -f "$preview_file" "$preview_changelog" "$preview_checksums"
    ralph_log_success "Release transaction contract is valid"
}
