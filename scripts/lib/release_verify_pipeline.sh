#!/usr/bin/env bash
#
# Purpose: Prepare and validate Ralph release verification snapshots.
# Responsibilities:
# - Run ship/public-readiness gates for the release snapshot.
# - Build artifacts and render release notes for the verified snapshot.
# - Record the exact local state that execute/reconcile must honor.
# Scope:
# - Local verification snapshot preparation only; remote publication lives elsewhere.
# Usage:
# - source "$(dirname "$0")/lib/release_verify_pipeline.sh"
# Invariants/assumptions:
# - Caller already sourced release policy, verify-state helpers, and changelog helpers.

if [ -n "${RALPH_RELEASE_VERIFY_PIPELINE_SOURCED:-}" ]; then
    return 0
fi
RALPH_RELEASE_VERIFY_PIPELINE_SOURCED=1
set -euo pipefail

release_run_ship_gate() {
    local make_cmd
    make_cmd=$(ralph_resolve_make_cmd)

    ralph_log_step "Running ship gate"
    cd "$REPO_ROOT"

    ralph_log_info "Running shared release gate"
    "$make_cmd" release-gate

    local collected_dirty_lines
    collected_dirty_lines=$(release_collect_dirty_lines "$REPO_ROOT") || return 1
    local dirty_lines
    dirty_lines=$(release_filter_dirty_lines "$collected_dirty_lines")
    if ! release_assert_dirty_paths_allowed "$dirty_lines"; then
        return 1
    fi

    ralph_log_success "Ship gate passed"
}

release_changelog_has_curated_unreleased_content() {
    local changelog="$1"
    awk '
        /^## \[Unreleased\]/ {
            in_unreleased = 1
            next
        }
        /^## \[/ && in_unreleased {
            exit
        }
        in_unreleased && $0 !~ /^[[:space:]]*$/ {
            found = 1
        }
        END {
            exit found ? 0 : 1
        }
    ' "$changelog"
}

release_generate_changelog_entries() {
    ralph_log_step "Generating changelog entries"
    cd "$REPO_ROOT"

    if release_changelog_has_curated_unreleased_content "$CHANGELOG"; then
        ralph_log_info "Preserving curated CHANGELOG.md Unreleased notes"
        return 0
    fi

    ./scripts/generate-changelog.sh
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
