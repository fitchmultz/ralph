# Purpose: Define Ralph repository safety and guardrail targets included by the root Makefile.
# Responsibilities: Own environment safety, backup-artifact checks, file-size checks, repo-safety aliases, and pre-commit composition.
# Scope: Target bodies only; public help text and phony aggregation stay in ../Makefile.
# Usage: Included by ../Makefile; invoke targets through the root Makefile rather than this fragment directly.
# Invariants/Assumptions: The repository scripts directory is available from the root working directory.

check-env-safety:
	@scripts/pre-public-check.sh --skip-ci --skip-links --skip-clean --allow-no-git

check-backup-artifacts:
	@bak_files="$$(find crates/ralph/src/ -name '*.bak' -type f 2>/dev/null || true)"; \
	if [ -n "$$bak_files" ]; then \
		echo "ERROR: Backup artifacts found in crates/ralph/src/:"; \
		echo "$$bak_files"; \
		echo "Remove these files before committing."; \
		exit 1; \
	fi

check-file-size-limits:
	@bash ./scripts/check-file-size-limits.sh

check-repo-safety: check-env-safety
	@true

pre-commit: check-env-safety check-backup-artifacts format-check
	@echo "→ Pre-commit checks complete"
	@echo "  ✓ Pre-commit checks passed"
