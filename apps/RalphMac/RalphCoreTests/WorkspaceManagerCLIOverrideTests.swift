/**
 WorkspaceManagerCLIOverrideTests

 Purpose:
 - Validate workspace-manager CLI override adoption rejects insecure or invalid URL-driven overrides.

 Responsibilities:
 - Validate workspace-manager CLI override adoption rejects insecure or invalid URL-driven overrides.

 Does not handle:
 - Runner-configuration loading or retargeting behavior.

 Usage:
 - Used by the RalphMac app or RalphCore tests through its owning feature surface.

 Invariants/assumptions callers must respect:
 - Tests assert current hard-cutover policy: URL-driven CLI overrides are not adopted.
 */

import XCTest

@testable import RalphCore

@MainActor
final class WorkspaceManagerCLIOverrideTests: RalphCoreTestCase {
    func test_workspaceManager_adoptCLIExecutable_rejectsValidPathOverride() async throws {
        let manager = WorkspaceManager.shared
        let baselinePath = manager.client?.executableURL.standardizedFileURL.resolvingSymlinksInPath().path
        let tempDir = try WorkspacePerformanceTestSupport.makeTempDir(prefix: "ralph-workspace-manager-cli")
        defer { RalphCoreTestSupport.assertRemoved(tempDir) }
        let overrideURL = try WorkspacePerformanceTestSupport.makeVersionAwareMockCLI(in: tempDir, name: "mock-ralph-version-ok")

        manager.adoptCLIExecutable(path: overrideURL.path)

        if let baselinePath {
            XCTAssertEqual(
                manager.client?.executableURL.standardizedFileURL.resolvingSymlinksInPath().path,
                baselinePath
            )
        } else {
            XCTAssertNil(manager.client)
        }
    }

    func test_workspaceManager_adoptCLIExecutable_preservesClientOnInvalidPath() {
        let manager = WorkspaceManager.shared
        let baselinePath = manager.client?.executableURL.standardizedFileURL.resolvingSymlinksInPath().path

        manager.adoptCLIExecutable(path: "/definitely/not/a/real/ralph-binary")

        if let baselinePath {
            XCTAssertEqual(
                manager.client?.executableURL.standardizedFileURL.resolvingSymlinksInPath().path,
                baselinePath
            )
        } else {
            XCTAssertNotNil(manager.errorMessage)
        }
    }
}
