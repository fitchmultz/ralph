# Purpose: Define Ralph macOS app build, test, UI evidence, and local ship-gate targets included by the root Makefile.
# Responsibilities: Own Xcode preflight, app build/install, XCTest lanes, UI retest workflows, artifacts, contract checks, and macOS CI orchestration.
# Scope: macOS/Xcode target bodies only; shared variables and help text stay in ../Makefile.
# Usage: Included by ../Makefile; invoke targets through the root Makefile rather than this fragment directly.
# Invariants/Assumptions: Xcode-related variables, release build stamp variables, shell flags, and lock paths are defined by the including Makefile.

macos-preflight:
	@os="$$(uname -s)"; \
	if [ "$$os" != "Darwin" ]; then \
		echo "macos-preflight: macOS-only (uname: $$os)"; \
		exit 1; \
	fi; \
	if ! command -v xcodebuild >/dev/null 2>&1; then \
		echo "macos-preflight: xcodebuild not found on PATH"; \
		exit 1; \
	fi

macos-build: macos-preflight $(RALPH_RELEASE_BUILD_STAMP)
	@lock_dir="$(XCODE_BUILD_LOCK_DIR)"; \
	source scripts/lib/xcodebuild-lock.sh; \
	acquired=0; \
	cleanup() { if [ "$$acquired" = "1" ]; then ralph_release_xcode_build_lock "$$lock_dir"; fi; }; \
	trap cleanup EXIT INT TERM; \
	ralph_acquire_xcode_build_lock "$$lock_dir" "macos-build"; \
	acquired=1; \
	derived_data_path="$(XCODE_MACOS_BUILD_DERIVED_DATA_PATH)"; \
	echo "→ macOS build (Xcode build)..."; \
	if [ "$${RALPH_XCODE_KEEP_DERIVED_DATA:-0}" != "1" ]; then rm -rf "$$derived_data_path" 2>/dev/null || true; fi; \
	xcodebuild \
		-project apps/RalphMac/RalphMac.xcodeproj \
		-scheme RalphMac \
		-configuration Release \
		-destination '$(XCODE_DESTINATION)' \
		-derivedDataPath "$$derived_data_path" \
		$(XCODE_JOBS_FLAG) \
		$(XCODE_ACTIVE_ARCH_FLAGS) \
		CODE_SIGNING_ALLOWED=NO CODE_SIGNING_REQUIRED=NO CODE_SIGN_IDENTITY="" \
		SWIFT_TREAT_WARNINGS_AS_ERRORS=YES GCC_TREAT_WARNINGS_AS_ERRORS=YES \
		build

macos-install-app: macos-build
	@derived_data_path="$(XCODE_MACOS_BUILD_DERIVED_DATA_PATH)"; \
	app_bundle="$$derived_data_path/Build/Products/Release/RalphMac.app"; \
	install_dir="$(MACOS_APP_INSTALL_DIR)"; \
	if [ ! -w "$$install_dir" ]; then \
		install_dir="$(HOME)/Applications"; \
		echo "macos-install-app: $(MACOS_APP_INSTALL_DIR) not writable; using $$install_dir"; \
	fi; \
	mkdir -p "$$install_dir"; \
	dest_bundle="$$install_dir/RalphMac.app"; \
	echo "→ Installing RalphMac.app to $$dest_bundle"; \
	rm -rf "$$dest_bundle"; \
	ditto "$$app_bundle" "$$dest_bundle"; \
	/System/Library/Frameworks/CoreServices.framework/Versions/Current/Frameworks/LaunchServices.framework/Versions/Current/Support/lsregister -f "$$dest_bundle" >/dev/null 2>&1 || true; \
	echo "  ✓ RalphMac.app installed"

macos-test: macos-preflight $(RALPH_RELEASE_BUILD_STAMP)
	@include_ui_tests="$(RALPH_UI_TESTS)"; \
	result_bundle_path="$(XCODE_RESULT_BUNDLE_PATH)"; \
	if [ "$$include_ui_tests" = "1" ]; then \
		echo "→ macOS tests (Xcode, including UI tests - will take over mouse/keyboard)..."; \
		$(MAKE) --no-print-directory macos-ui-build-for-testing; \
		$(MAKE) --no-print-directory macos-ui-retest \
			RALPH_UI_SCREENSHOTS="$(RALPH_UI_SCREENSHOTS)" \
			RALPH_UI_SCREENSHOT_MODE="$(RALPH_UI_SCREENSHOT_MODE)" \
			XCODE_RESULT_BUNDLE_PATH="$$result_bundle_path"; \
	else \
		lock_dir="$(XCODE_BUILD_LOCK_DIR)"; \
		source scripts/lib/xcodebuild-lock.sh; \
		acquired=0; \
		cleanup() { if [ "$$acquired" = "1" ]; then ralph_release_xcode_build_lock "$$lock_dir"; fi; }; \
		trap cleanup EXIT INT TERM; \
		derived_data_path="$(XCODE_MACOS_TEST_DERIVED_DATA_PATH)"; \
		ralph_acquire_xcode_build_lock "$$lock_dir" "macos-test"; \
		acquired=1; \
		echo "→ macOS tests (Xcode, skipping UI tests - use RALPH_UI_TESTS=1 to include)..."; \
		skipped_tests="-skip-testing RalphMacUITests"; \
		if [ "$${RALPH_XCODE_KEEP_DERIVED_DATA:-0}" != "1" ]; then rm -rf "$$derived_data_path" 2>/dev/null || true; fi; \
		xcodebuild \
			-project apps/RalphMac/RalphMac.xcodeproj \
			-scheme RalphMac \
			-configuration Debug \
			-destination '$(XCODE_DESTINATION)' \
			-derivedDataPath "$$derived_data_path" \
			$(XCODE_JOBS_FLAG) \
			$(XCODE_ACTIVE_ARCH_FLAGS) \
			CODE_SIGNING_ALLOWED=NO CODE_SIGNING_REQUIRED=NO CODE_SIGN_IDENTITY="" \
			SWIFT_TREAT_WARNINGS_AS_ERRORS=YES GCC_TREAT_WARNINGS_AS_ERRORS=YES \
			$$skipped_tests \
			test; \
	fi; \
	true

# Build/sign macOS UI test bundles once for local iteration.
# Use macos-ui-retest repeatedly afterward to avoid fresh bundle preparation.
macos-ui-build-for-testing: macos-preflight $(RALPH_RELEASE_BUILD_STAMP)
	@lock_dir="$(XCODE_BUILD_LOCK_DIR)"; \
	source scripts/lib/xcodebuild-lock.sh; \
	acquired=0; \
	cleanup() { if [ "$$acquired" = "1" ]; then ralph_release_xcode_build_lock "$$lock_dir"; fi; }; \
	trap cleanup EXIT INT TERM; \
	ralph_acquire_xcode_build_lock "$$lock_dir" "macos-ui-build-for-testing"; \
	acquired=1; \
	derived_data_path="$(XCODE_DERIVED_DATA_ROOT)/ui"; \
	echo "→ macOS UI build-for-testing (one-time prompt may appear for a rebuilt bundle)..."; \
	rm -rf "$$derived_data_path" 2>/dev/null || true; \
	xcodebuild \
		-project apps/RalphMac/RalphMac.xcodeproj \
		-scheme RalphMac \
		-configuration Debug \
		-destination '$(XCODE_DESTINATION)' \
		-derivedDataPath "$$derived_data_path" \
		$(XCODE_JOBS_FLAG) \
		$(XCODE_ACTIVE_ARCH_FLAGS) \
		CODE_SIGNING_ALLOWED=NO CODE_SIGNING_REQUIRED=NO CODE_SIGN_IDENTITY="" \
		SWIFT_TREAT_WARNINGS_AS_ERRORS=YES GCC_TREAT_WARNINGS_AS_ERRORS=YES \
		build-for-testing; \
	echo "→ Clearing quarantine metadata on UI test bundles..."; \
	xattr -dr com.apple.quarantine "$$derived_data_path/Build/Products/Debug/RalphMac.app" "$$derived_data_path/Build/Products/Debug/RalphMacUITests-Runner.app" 2>/dev/null || true; \
	echo "→ Re-signing UI test bundles (ad-hoc) to avoid Gatekeeper runner failures..."; \
	codesign --force --deep --sign - "$$derived_data_path/Build/Products/Debug/RalphMac.app"; \
	codesign --force --deep --sign - "$$derived_data_path/Build/Products/Debug/RalphMacUITests-Runner.app"; \
	echo "  ✓ Prepared UI runner under $$derived_data_path"

# Re-run macOS UI tests without rebuilding the app/runner bundles.
# Optional: set RALPH_UI_ONLY_TESTING=<Target/Class/testMethod> to focus a single test.
macos-ui-retest:
	@lock_dir="$(XCODE_BUILD_LOCK_DIR)"; \
	source scripts/lib/xcodebuild-lock.sh; \
	acquired=0; \
	derived_data_path="$(XCODE_DERIVED_DATA_ROOT)/ui"; \
	app_binary="$$derived_data_path/Build/Products/Debug/RalphMac.app/Contents/MacOS/RalphMac"; \
	runner_binary="$$derived_data_path/Build/Products/Debug/RalphMacUITests-Runner.app/Contents/MacOS/RalphMacUITests-Runner"; \
	cleanup() { \
		if pgrep -f "$$runner_binary" >/dev/null 2>&1; then pkill -TERM -f "$$runner_binary" >/dev/null 2>&1 || true; sleep 1; pgrep -f "$$runner_binary" >/dev/null 2>&1 && pkill -KILL -f "$$runner_binary" >/dev/null 2>&1 || true; fi; \
		if pgrep -f "$$app_binary" >/dev/null 2>&1; then pkill -TERM -f "$$app_binary" >/dev/null 2>&1 || true; sleep 1; pgrep -f "$$app_binary" >/dev/null 2>&1 && pkill -KILL -f "$$app_binary" >/dev/null 2>&1 || true; fi; \
		if [ "$$acquired" = "1" ]; then ralph_release_xcode_build_lock "$$lock_dir"; fi; \
	}; \
	trap cleanup EXIT INT TERM; \
	ralph_acquire_xcode_build_lock "$$lock_dir" "macos-ui-retest"; \
	acquired=1; \
	derived_data_path="$(XCODE_DERIVED_DATA_ROOT)/ui"; \
	result_bundle_path="$(XCODE_RESULT_BUNDLE_PATH)"; \
	only_testing="$(RALPH_UI_ONLY_TESTING)"; \
	app_bundle="$$derived_data_path/Build/Products/Debug/RalphMac.app"; \
	runner_bundle="$$derived_data_path/Build/Products/Debug/RalphMacUITests-Runner.app"; \
	if [ ! -d "$$app_bundle" ] || [ ! -d "$$runner_bundle" ]; then \
		echo "ERROR: UI test bundles are not prepared. Run 'make macos-ui-build-for-testing' first." >&2; \
		exit 2; \
	fi; \
	result_bundle_args=(); \
	if [ -n "$$result_bundle_path" ]; then \
		mkdir -p "$$(dirname "$$result_bundle_path")"; \
		result_bundle_args=(-resultBundlePath "$$result_bundle_path"); \
	fi; \
	test_scope_args=(); \
	if [ -n "$$only_testing" ]; then \
		test_scope_args=(-only-testing:"$$only_testing"); \
		echo "→ macOS UI retest (targeted: $$only_testing)..."; \
	else \
		echo "→ macOS UI retest (reusing prepared bundles; no rebuild)..."; \
	fi; \
	RALPH_UI_SCREENSHOTS="$(RALPH_UI_SCREENSHOTS)" \
	RALPH_UI_SCREENSHOT_MODE="$(RALPH_UI_SCREENSHOT_MODE)" \
	xcodebuild \
		-project apps/RalphMac/RalphMac.xcodeproj \
		-scheme RalphMac \
		-configuration Debug \
		-destination '$(XCODE_DESTINATION)' \
		-derivedDataPath "$$derived_data_path" \
		$(XCODE_JOBS_FLAG) \
		$(XCODE_ACTIVE_ARCH_FLAGS) \
		"$${result_bundle_args[@]}" \
		"$${test_scope_args[@]}" \
		test-without-building; \
	if pgrep -f "$$runner_binary" >/dev/null 2>&1 || pgrep -f "$$app_binary" >/dev/null 2>&1; then \
		echo "ERROR: macos-ui-retest left a lingering UI test app or runner process" >&2; \
		ps -axo pid=,command= | grep -E "$$runner_binary|$$app_binary" | grep -v grep >&2 || true; \
		exit 1; \
	fi

# Run macOS UI tests (interactive - will take over mouse/keyboard)
macos-test-ui:
	@$(MAKE) --no-print-directory macos-ui-build-for-testing
	@$(MAKE) --no-print-directory macos-ui-retest

# Run macOS UI tests with preserved xcresult output (interactive).
# Stores timestamped artifacts under $(RALPH_UI_ARTIFACTS_ROOT)/<timestamp>/.
macos-test-ui-artifacts: macos-preflight $(RALPH_RELEASE_BUILD_STAMP)
	@timestamp="$$(date +%Y%m%d-%H%M%S)"; \
	artifact_dir="$(RALPH_UI_ARTIFACTS_ROOT)/$$timestamp"; \
	result_bundle_path="$$artifact_dir/RalphMacUITests.xcresult"; \
	summary_path="$$artifact_dir/summary.txt"; \
	mkdir -p "$$artifact_dir"; \
	echo "→ macOS UI tests with xcresult capture..."; \
	set +e; \
	$(MAKE) --no-print-directory macos-ui-build-for-testing; \
	$(MAKE) --no-print-directory macos-ui-retest \
		XCODE_RESULT_BUNDLE_PATH="$$result_bundle_path"; \
	test_exit="$$?"; \
	set -e; \
	final_exit="$$test_exit"; \
	if [ -d "$$result_bundle_path" ]; then \
		{ \
			echo "Ralph macOS UI artifact summary"; \
			echo "timestamp: $$timestamp"; \
			echo "result_bundle: $$result_bundle_path"; \
			echo "targeted_test: $${RALPH_UI_ONLY_TESTING:-all}"; \
		} > "$$summary_path"; \
		echo "  ✓ Result bundle: $$result_bundle_path"; \
		echo "  ✓ Summary: $$summary_path"; \
	else \
		echo "  ⚠ No xcresult bundle found at $$result_bundle_path"; \
		if [ "$$test_exit" = "0" ]; then final_exit=1; fi; \
	fi; \
	echo "  ℹ Cleanup after review: make macos-ui-artifacts-clean"; \
	exit "$$final_exit"

# Remove captured UI visual artifacts after review.
macos-ui-artifacts-clean:
	@echo "→ Removing captured UI visual artifacts..."
	@rm -rf "$(RALPH_UI_ARTIFACTS_ROOT)"
	@echo "  ✓ UI visual artifacts removed"

# Run deterministic non-XCTest macOS contract checks against the built app.
macos-test-contracts: macos-test-settings-smoke macos-test-workspace-routing-contract
	@echo "→ macOS deterministic contract checks completed"

# Run targeted noninteractive Settings contract coverage for supported entry paths.
macos-test-settings-smoke: macos-build
	@echo "→ macOS Settings smoke contract coverage (keyboard, app menu, URL route; noninteractive)..."
	@./scripts/macos-settings-smoke.sh --app-bundle "$(XCODE_MACOS_RELEASE_APP_BUNDLE)"

# Run targeted noninteractive workspace bootstrap/routing contract coverage.
macos-test-workspace-routing-contract: macos-build
	@echo "→ macOS workspace routing contract coverage (bootstrap, URL open, pending scene routes; noninteractive)..."
	@./scripts/macos-workspace-routing-contract.sh --app-bundle "$(XCODE_MACOS_RELEASE_APP_BUNDLE)"

macos-test-window-shortcuts: macos-preflight $(RALPH_RELEASE_BUILD_STAMP)
	@lock_dir="$(XCODE_BUILD_LOCK_DIR)"; \
	source scripts/lib/xcodebuild-lock.sh; \
	acquired=0; \
	derived_data_path="$(XCODE_DERIVED_DATA_ROOT)/ui-shortcuts"; \
	app_binary="$$derived_data_path/Build/Products/Debug/RalphMac.app/Contents/MacOS/RalphMac"; \
	runner_binary="$$derived_data_path/Build/Products/Debug/RalphMacUITests-Runner.app/Contents/MacOS/RalphMacUITests-Runner"; \
	cleanup() { \
		if pgrep -f "$$runner_binary" >/dev/null 2>&1; then pkill -TERM -f "$$runner_binary" >/dev/null 2>&1 || true; sleep 1; pgrep -f "$$runner_binary" >/dev/null 2>&1 && pkill -KILL -f "$$runner_binary" >/dev/null 2>&1 || true; fi; \
		if pgrep -f "$$app_binary" >/dev/null 2>&1; then pkill -TERM -f "$$app_binary" >/dev/null 2>&1 || true; sleep 1; pgrep -f "$$app_binary" >/dev/null 2>&1 && pkill -KILL -f "$$app_binary" >/dev/null 2>&1 || true; fi; \
		if [ "$$acquired" = "1" ]; then ralph_release_xcode_build_lock "$$lock_dir"; fi; \
	}; \
	trap cleanup EXIT INT TERM; \
	ralph_acquire_xcode_build_lock "$$lock_dir" "macos-test-window-shortcuts"; \
	acquired=1; \
	derived_data_path="$(XCODE_DERIVED_DATA_ROOT)/ui-shortcuts"; \
	echo "→ macOS UI shortcut regressions (focused window/tab behavior)..."; \
	rm -rf "$$derived_data_path" 2>/dev/null || true; \
	xcodebuild \
		-project apps/RalphMac/RalphMac.xcodeproj \
		-scheme RalphMac \
		-configuration Debug \
		-destination '$(XCODE_DESTINATION)' \
		-derivedDataPath "$$derived_data_path" \
		$(XCODE_JOBS_FLAG) \
		$(XCODE_ACTIVE_ARCH_FLAGS) \
		CODE_SIGNING_ALLOWED=NO CODE_SIGNING_REQUIRED=NO CODE_SIGN_IDENTITY="" \
		SWIFT_TREAT_WARNINGS_AS_ERRORS=YES GCC_TREAT_WARNINGS_AS_ERRORS=YES \
		-only-testing:RalphMacUITests/RalphMacUIWindowRoutingTests/test_windowShortcuts_affectOnlyFocusedWindow \
		-only-testing:RalphMacUITests/RalphMacUIWindowRoutingTests/test_commandPaletteNewTab_affectsOnlyFocusedWindow \
		build-for-testing; \
	echo "→ Clearing quarantine metadata on UI test bundles..."; \
	xattr -dr com.apple.quarantine "$$derived_data_path/Build/Products/Debug/RalphMac.app" "$$derived_data_path/Build/Products/Debug/RalphMacUITests-Runner.app" 2>/dev/null || true; \
	echo "→ Re-signing UI test bundles (ad-hoc) to avoid Gatekeeper runner failures..."; \
	codesign --force --deep --sign - "$$derived_data_path/Build/Products/Debug/RalphMac.app"; \
	codesign --force --deep --sign - "$$derived_data_path/Build/Products/Debug/RalphMacUITests-Runner.app"; \
	xcodebuild \
		-project apps/RalphMac/RalphMac.xcodeproj \
		-scheme RalphMac \
		-configuration Debug \
		-destination '$(XCODE_DESTINATION)' \
		-derivedDataPath "$$derived_data_path" \
		$(XCODE_JOBS_FLAG) \
		$(XCODE_ACTIVE_ARCH_FLAGS) \
		-only-testing:RalphMacUITests/RalphMacUIWindowRoutingTests/test_windowShortcuts_affectOnlyFocusedWindow \
		-only-testing:RalphMacUITests/RalphMacUIWindowRoutingTests/test_commandPaletteNewTab_affectsOnlyFocusedWindow \
		test-without-building; \
	if pgrep -f "$$runner_binary" >/dev/null 2>&1 || pgrep -f "$$app_binary" >/dev/null 2>&1; then \
		echo "ERROR: macos-test-window-shortcuts left a lingering UI test app or runner process" >&2; \
		ps -axo pid=,command= | grep -E "$$runner_binary|$$app_binary" | grep -v grep >&2 || true; \
		exit 1; \
	fi

macos-ci: macos-preflight
	@shared_derived_data_path="$(XCODE_DERIVED_DATA_ROOT)/ship"; \
	keep_derived_data="$${RALPH_XCODE_KEEP_DERIVED_DATA:-0}"; \
	run_dir="$$(mktemp -d "$${TMPDIR:-/tmp}/ralph-macos-ci.XXXXXX")"; \
	rust_log="$$run_dir/rust-ci.log"; \
	macos_log="$$run_dir/macos-validation.log"; \
	rust_pid=""; \
	macos_pid=""; \
	cleanup() { \
		status="$$?"; \
		trap - EXIT INT TERM; \
		if [ "$$status" -ne 0 ]; then \
			for child_pid in $$rust_pid $$macos_pid; do \
				if [ -n "$$child_pid" ] && kill -0 "$$child_pid" 2>/dev/null; then \
					kill "$$child_pid" 2>/dev/null || true; \
				fi; \
			done; \
			for child_pid in $$rust_pid $$macos_pid; do \
				if [ -n "$$child_pid" ]; then wait "$$child_pid" 2>/dev/null || true; fi; \
			done; \
		fi; \
		if [ "$$keep_derived_data" != "1" ]; then rm -rf "$$shared_derived_data_path" 2>/dev/null || true; fi; \
		rm -rf "$$run_dir" 2>/dev/null || true; \
		exit "$$status"; \
	}; \
	trap cleanup EXIT INT TERM; \
	if [ "$$keep_derived_data" != "1" ]; then rm -rf "$$shared_derived_data_path" 2>/dev/null || true; fi; \
	echo "→ macOS ship gate (prebuilding shared release CLI stamp)..."; \
	$(MAKE) --no-print-directory build; \
	echo "→ macOS ship gate (running Rust CI and macOS validation lanes concurrently)..."; \
	( $(MAKE) --no-print-directory ci ) >"$$rust_log" 2>&1 & \
	rust_pid="$$!"; \
	( $(MAKE) --no-print-directory macos-build macos-test macos-test-contracts \
		RALPH_XCODE_REUSE_SHIP_DERIVED_DATA=1 \
		RALPH_XCODE_KEEP_DERIVED_DATA=1 ) >"$$macos_log" 2>&1 & \
	macos_pid="$$!"; \
	set +e; \
	wait "$$rust_pid"; \
	rust_status="$$?"; \
	wait "$$macos_pid"; \
	macos_status="$$?"; \
	set -e; \
	echo ""; \
	echo "== Rust CI lane output =="; \
	cat "$$rust_log"; \
	echo "== End Rust CI lane output =="; \
	echo ""; \
	echo "== macOS validation lane output =="; \
	cat "$$macos_log"; \
	echo "== End macOS validation lane output =="; \
	if [ "$$rust_status" -ne 0 ] || [ "$$macos_status" -ne 0 ]; then \
		echo ""; \
		echo "macos-ci: lane failure summary:" >&2; \
		if [ "$$rust_status" -ne 0 ]; then echo "  ✗ Rust CI lane failed with exit $$rust_status" >&2; fi; \
		if [ "$$macos_status" -ne 0 ]; then echo "  ✗ macOS validation lane failed with exit $$macos_status" >&2; fi; \
		exit 1; \
	fi; \
	echo "→ macOS ship gate (Rust CI + macOS app build+test + deterministic contract smoke)..."; \
	echo "  ℹ Interactive XCTest UI automation remains excluded from macos-ci (use make macos-test-ui or make macos-test-window-shortcuts when idle)."; \
	echo "  ✓ macOS CI completed"
