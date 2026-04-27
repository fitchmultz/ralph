# Purpose: Define Ralph coverage report targets included by the root Makefile.
# Responsibilities: Own coverage output configuration, report generation, and coverage artifact cleanup.
# Scope: Coverage target bodies only; public help text and phony aggregation stay in ../Makefile.
# Usage: Included by ../Makefile; invoke targets through the root Makefile rather than this fragment directly.
# Invariants/Assumptions: cargo-llvm-cov and jq are optional operator tools discovered at recipe runtime.

COVERAGE_DIR ?= target/coverage

# Coverage: Generate HTML and summary reports (requires cargo-llvm-cov)
# Generates: HTML report, text summary with per-crate breakdown, and JSON data
coverage:
	@echo "→ Running coverage analysis..."
	@if ! cargo llvm-cov --version >/dev/null 2>&1; then \
		echo "ERROR: cargo-llvm-cov not found."; \
		echo ""; \
		echo "Install with:"; \
		echo "  cargo install cargo-llvm-cov"; \
		echo ""; \
		echo "On macOS, you may also need:"; \
		echo "  rustup component add llvm-tools-preview"; \
		exit 1; \
	fi
	@mkdir -p $(COVERAGE_DIR)
	@echo "  → Running tests with coverage instrumentation..."
	@cargo llvm-cov --workspace --all-targets --all-features --locked \
		--html --output-dir $(COVERAGE_DIR)/html \
		--json --output-path $(COVERAGE_DIR)/coverage.json
	@echo ""
	@echo "  ✓ Coverage report generated:"
	@echo "    HTML:  $(COVERAGE_DIR)/html/index.html"
	@echo "    JSON:  $(COVERAGE_DIR)/coverage.json"
	@echo ""
	@echo "  → Coverage summary:"
	@echo ""
	@echo "    Total Coverage:"
	@jq -r '[.data[0].totals.lines.percent // 0, .data[0].totals.functions.percent // 0, .data[0].totals.regions.percent // 0] | "      Lines: \(.[0])%, Functions: \(.[1])%, Regions: \(.[2])%"' $(COVERAGE_DIR)/coverage.json 2>/dev/null || echo "      (install jq for formatted output)"
	@echo ""
	@echo "    Per-Crate Breakdown:"
	@jq -r '.data[0].summaries // [] | sort_by(.crate_name) | .[] | "      \(.crate_name): Lines \(.summary.lines.percent // 0)%, Functions \(.summary.functions.percent // 0)%"' $(COVERAGE_DIR)/coverage.json 2>/dev/null || echo "      (see $(COVERAGE_DIR)/coverage.json for raw data)"
	@echo ""
	@echo "  → Opening HTML report..."
	@open $(COVERAGE_DIR)/html/index.html 2>/dev/null || echo "    (open $(COVERAGE_DIR)/html/index.html manually)"

# Coverage clean: Remove coverage artifacts
coverage-clean:
	@echo "→ Cleaning coverage artifacts..."
	@rm -rf $(COVERAGE_DIR)
	@find . -name '*.profraw' -type f -delete 2>/dev/null || true
	@find . -name '*.profdata' -type f -delete 2>/dev/null || true
	@echo "  ✓ Coverage artifacts removed"
