# Purpose: Provide a portable `make` entrypoint that delegates to GNU Make.
# Responsibilities: Let macOS/BSD `make <target>` work by re-executing the repository's GNU Make build surface.
# Scope: Compatibility shim only; canonical targets live in `mk/root.mk` and included `mk/*.mk` fragments.
# Usage: Run `make <target>` or `gmake -f mk/root.mk <target>` from the repository root.
# Invariants/Assumptions: GNU Make is available as `gmake` or as Homebrew's gnubin `make` when this shim is reached.

.PHONY: gnu-make-dispatch help install install-verify macos-install-app update lint lint-fix format format-check type-check clean clean-temp test generate docs build ci ci-fast ci-docs deps changelog changelog-preview changelog-check version-check version-sync publish-check release release-dry-run release-verify release-artifacts pre-commit pre-public-check release-gate profile-ship-gate profile-ship-gate-clean agent-ci check-env-safety check-backup-artifacts check-file-size-limits check-repo-safety macos-preflight macos-build macos-test macos-ci macos-test-ui macos-ui-build-for-testing macos-ui-retest macos-test-ui-artifacts macos-ui-artifacts-clean macos-test-window-shortcuts macos-test-contracts macos-test-settings-smoke macos-test-workspace-routing-contract coverage coverage-clean

help install install-verify macos-install-app update lint lint-fix format format-check type-check clean clean-temp test generate docs build ci ci-fast ci-docs deps changelog changelog-preview changelog-check version-check version-sync publish-check release release-dry-run release-verify release-artifacts pre-commit pre-public-check release-gate profile-ship-gate profile-ship-gate-clean agent-ci check-env-safety check-backup-artifacts check-file-size-limits check-repo-safety macos-preflight macos-build macos-test macos-ci macos-test-ui macos-ui-build-for-testing macos-ui-retest macos-test-ui-artifacts macos-ui-artifacts-clean macos-test-window-shortcuts macos-test-contracts macos-test-settings-smoke macos-test-workspace-routing-contract coverage coverage-clean: gnu-make-dispatch

gnu-make-dispatch:
	@if command -v gmake >/dev/null 2>&1; then \
		exec gmake -f mk/root.mk $(MAKECMDGOALS); \
	fi; \
	if command -v /opt/homebrew/opt/make/libexec/gnubin/make >/dev/null 2>&1; then \
		exec /opt/homebrew/opt/make/libexec/gnubin/make -f mk/root.mk $(MAKECMDGOALS); \
	fi; \
	if command -v /usr/local/opt/make/libexec/gnubin/make >/dev/null 2>&1; then \
		exec /usr/local/opt/make/libexec/gnubin/make -f mk/root.mk $(MAKECMDGOALS); \
	fi; \
	echo "GNU Make >= 4 is required. On macOS: brew install make, then run gmake -f mk/root.mk <target> or add Homebrew gnubin to PATH." >&2; \
	exit 2
