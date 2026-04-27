# Purpose: Define Ralph local CI and release-gate targets included by the root Makefile.
# Responsibilities: Own docs, fast Rust, full Rust, release, profile, and diff-routed agent CI gates.
# Scope: Target bodies only; lower-level build/test/check targets are owned by sibling fragments.
# Usage: Included by ../Makefile; invoke targets through the root Makefile rather than this fragment directly.
# Invariants/Assumptions: Safety, Rust, and macOS targets referenced here are defined by included fragments before make executes a recipe.

ci-docs: check-env-safety check-backup-artifacts check-file-size-limits
	@echo "→ Docs-only CI gate (no executable surface changed)..."
	@bash ./scripts/lib/public_readiness_scan.sh docs
	@echo ""
	@echo "  ✓ Docs-only CI completed"

# Fast deterministic Rust/CLI gate for routine development and PR-equivalent checks.
# Clippy is run with --all-targets/--all-features and type-checks the same Rust surface.
ci-fast: check-env-safety check-backup-artifacts check-file-size-limits deps format-check lint test
	@echo "→ Fast CI gate (format-check/lint/test)..."
	@echo ""
	@echo "  ✓ Fast CI completed"

# Full Rust release gate (includes release build/schema generation/CLI install verification).
ci: ci-fast build generate install-verify
	@echo "→ Full CI gate (ci-fast + release build/generate/install verification)..."
	@echo ""
	@echo "  ✓ CI completed"

release-gate:
	@if [ "$$(uname -s)" = "Darwin" ] && command -v xcodebuild >/dev/null 2>&1; then \
		echo "  → Running macOS release gate"; \
		$(MAKE) --no-print-directory macos-ci; \
	else \
		echo "  → Running Rust release gate"; \
		$(MAKE) --no-print-directory ci; \
	fi

profile-ship-gate: macos-preflight
	@bash scripts/profile-ship-gate.sh run

profile-ship-gate-clean:
	@bash scripts/profile-ship-gate.sh clean

# Agent CI: route to the smallest valid gate for the current local working-tree diff.
# Set RALPH_AGENT_CI_FORCE_MACOS=1 to force macos-ci. Optional RALPH_AGENT_CI_MIN_TIER raises the floor.
agent-ci:
	@echo "→ Agent CI gate (current local diff routing: docs, fast Rust, full Rust release, macOS ship)..."
	@force_macos="$${RALPH_AGENT_CI_FORCE_MACOS:-0}"; \
	if [ "$$force_macos" = "1" ]; then \
		echo "  → RALPH_AGENT_CI_FORCE_MACOS=1; running macOS gate"; \
		$(MAKE) --no-print-directory macos-ci; \
		exit 0; \
	fi; \
	if ! git rev-parse --is-inside-work-tree >/dev/null 2>&1; then \
		echo "  → Not in a git worktree; using platform-aware release gate fallback"; \
		$(MAKE) --no-print-directory release-gate; \
		exit 0; \
	fi; \
	eval "$$(scripts/agent-ci-surface.sh --emit-eval)"; \
	target_name="$$RALPH_AGENT_CI_TARGET"; \
	if [ "$$target_name" = "noop" ]; then \
		echo "  → $$RALPH_AGENT_CI_REASON"; \
		echo "  ✓ No local changes; nothing to validate"; \
		exit 0; \
	fi; \
	min_tier="$${RALPH_AGENT_CI_MIN_TIER:-}"; \
	if [ -n "$$min_tier" ]; then \
		case "$$min_tier" in \
			macos-ci) \
				case "$$target_name" in ci-docs|ci-fast|ci) target_name=macos-ci ;; esac \
				;; \
			ci) \
				case "$$target_name" in ci-docs|ci-fast) target_name=ci ;; esac \
				;; \
			ci-fast) \
				case "$$target_name" in ci-docs) target_name=ci-fast ;; esac \
				;; \
			*) \
				echo "  → ERROR: unknown RALPH_AGENT_CI_MIN_TIER=$$min_tier (use macos-ci, ci, or ci-fast)" >&2; \
				exit 2 \
				;; \
		esac; \
	fi; \
	echo "  → $$RALPH_AGENT_CI_REASON"; \
	$(MAKE) --no-print-directory "$$target_name"
