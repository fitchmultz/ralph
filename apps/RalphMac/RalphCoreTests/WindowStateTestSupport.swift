/**
 WindowStateTestSupport

 Purpose:
 - Provide shared setup, teardown, and filesystem/defaults helpers for split window-state suites.

 Responsibilities:
 - Provide shared setup, teardown, and filesystem/defaults helpers for split window-state suites.

 Does not handle:
 - Defining assertions for specific persistence or routing behaviors.

 Usage:
 - Used by the RalphMac app or RalphCore tests through its owning feature surface.

 Invariants/assumptions callers must respect:
 - Tests must run on the main actor because WorkspaceManager is main-actor isolated.
 - Each test starts with no open workspaces, no claimed window state, and cleared navigation/workspace defaults.
 */

import Foundation
import XCTest
@testable import RalphCore

@MainActor
class WindowStateTestCase: RalphCoreTestCase {
    let testRestorationKey = "com.mitchfultz.ralph.windowRestorationState"
    let testNavigationKey = "com.mitchfultz.ralph.navigationState"
    let workspaceSnapshotPrefix = "com.mitchfultz.ralph.workspace."

    var manager: WorkspaceManager!
    var defaults: UserDefaults { RalphAppDefaults.userDefaults }

    override func setUp() async throws {
        try await super.setUp()
        manager = WorkspaceManager.shared
        resetWindowStateEnvironment()
    }

    override func tearDown() async throws {
        resetWindowStateEnvironment()
        try await super.tearDown()
    }

    func workspaceSnapshotKey(for workspaceID: UUID) -> String {
        workspaceSnapshotPrefix + workspaceID.uuidString + ".snapshot"
    }

    func makeWorkspaceDirectory(prefix: String) throws -> URL {
        try RalphCoreTestSupport.makeTemporaryDirectory(prefix: prefix)
    }

    func makeSeededWorkspaceDirectory(prefix: String) throws -> URL {
        let directory = try makeWorkspaceDirectory(prefix: prefix)
        let ralphDirectory = directory.appendingPathComponent(".ralph", isDirectory: true)
        try FileManager.default.createDirectory(at: ralphDirectory, withIntermediateDirectories: true)
        try #"{"version":1,"tasks":[]}"#.write(
            to: ralphDirectory.appendingPathComponent("queue.jsonc", isDirectory: false),
            atomically: true,
            encoding: .utf8
        )
        return directory
    }

    private func resetWindowStateEnvironment() {
        manager.resetWindowStateClaimPool()
        manager.resetSceneRoutingForTests()
        closeAllWorkspaces()
        defaults.removeObject(forKey: testRestorationKey)
        cleanupNavigationState()
        cleanupWorkspaceSnapshots()
    }

    private func closeAllWorkspaces() {
        for workspace in manager.workspaces {
            manager.closeWorkspace(workspace)
        }
    }

    private func cleanupNavigationState() {
        for key in defaults.dictionaryRepresentation().keys where key.hasPrefix(testNavigationKey) {
            defaults.removeObject(forKey: key)
        }
    }

    private func cleanupWorkspaceSnapshots() {
        for key in defaults.dictionaryRepresentation().keys where key.hasPrefix(workspaceSnapshotPrefix) {
            defaults.removeObject(forKey: key)
        }
    }
}
