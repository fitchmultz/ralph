/**
 WorkspaceManagerVersioningTests

 Purpose:
 - Verify WorkspaceManager version-check flow enforces machine-contract versions for `machine system info`.

 Responsibilities:
 - Verify matching `machine system info` payloads continue through semantic version validation.
 - Verify unsupported `machine system info` versions fail fast with version-mismatch recovery messaging.
 - Keep shared WorkspaceManager singleton state isolated between tests.

 Scope:
 - WorkspaceManager version-check behavior only.

 Usage:
 - Used by the RalphMac app or RalphCore tests through its owning feature surface.

 Invariants/Assumptions:
 - Tests run on the main actor because `WorkspaceManager` is main-actor isolated.
 - Tests must restore shared singleton state before returning.
 */

import Foundation
import XCTest

@testable import RalphCore

@MainActor
final class WorkspaceManagerVersioningTests: RalphCoreTestCase {
    private func resetManagerVersioningState(_ manager: WorkspaceManager) {
        manager.versionCheckTask?.cancel()
        manager.versionCheckTask = nil
        manager.versionCheckResult = nil
        manager.errorMessage = nil
        RalphAppDefaults.userDefaults.removeObject(forKey: manager.versionCheckCacheKey)
    }

    func testExecuteVersionCheck_acceptsMatchingSystemInfoVersion() async throws {
        let manager = WorkspaceManager.shared
        let originalClient = manager.client
        resetManagerVersioningState(manager)
        defer {
            manager.client = originalClient
            resetManagerVersioningState(manager)
        }

        let tempDir = try RalphCoreTestSupport.makeTemporaryDirectory(prefix: "ralph-manager-version-check-ok")
        defer { RalphCoreTestSupport.assertRemoved(tempDir) }

        let script = """
        #!/bin/sh
        if [ "$1" = "--no-color" ] && [ "$2" = "machine" ] && [ "$3" = "system" ] && [ "$4" = "info" ]; then
          echo '{"version":1,"cli_version":"\(VersionCompatibility.minimumCLIVersion)"}'
          exit 0
        fi
        exit 64
        """
        let scriptURL = try RalphMockCLITestSupport.makeExecutableScript(in: tempDir, body: script)
        manager.client = try RalphCLIClient(executableURL: scriptURL)

        let result = await manager.executeVersionCheck()

        XCTAssertEqual(result?.status, .compatible)
        XCTAssertEqual(result?.rawVersion, VersionCompatibility.minimumCLIVersion)
        XCTAssertNil(manager.errorMessage)
    }

    func testExecuteVersionCheck_rejectsUnsupportedSystemInfoVersion() async throws {
        let manager = WorkspaceManager.shared
        let originalClient = manager.client
        resetManagerVersioningState(manager)
        defer {
            manager.client = originalClient
            resetManagerVersioningState(manager)
        }

        let tempDir = try RalphCoreTestSupport.makeTemporaryDirectory(prefix: "ralph-manager-version-check-mismatch")
        defer { RalphCoreTestSupport.assertRemoved(tempDir) }

        let script = """
        #!/bin/sh
        if [ "$1" = "--no-color" ] && [ "$2" = "machine" ] && [ "$3" = "system" ] && [ "$4" = "info" ]; then
          echo '{"version":999,"cli_version":"\(VersionCompatibility.minimumCLIVersion)"}'
          exit 0
        fi
        exit 64
        """
        let scriptURL = try RalphMockCLITestSupport.makeExecutableScript(in: tempDir, body: script)
        manager.client = try RalphCLIClient(executableURL: scriptURL)

        let result = await manager.executeVersionCheck()

        XCTAssertNil(result)
        XCTAssertTrue(manager.errorMessage?.contains("Unsupported machine system info version 999") == true)
        XCTAssertTrue(manager.errorMessage?.contains("Rebuild RalphMac and the bundled CLI from the same revision.") == true)
        XCTAssertFalse(manager.errorMessage?.contains("Failed to check CLI version:") == true)
    }
}
