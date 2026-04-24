/**
 WindowStateRestorationTests

 Purpose:
 - Validate window restoration fallback behavior, claim selection, and workspace identity helpers.

 Responsibilities:
 - Validate window restoration fallback behavior, claim selection, and workspace identity helpers.
 - Cover startup-placeholder semantics used during URL-routing bootstrap.

 Does not handle:
 - Scene routing dispatch across windows.
 - App-default pruning and frame cleanup.

 Usage:
 - Used by the RalphMac app or RalphCore tests through its owning feature surface.

 Invariants/assumptions callers must respect:
 - Each test uses disposable workspaces from the shared WindowStateTestCase helpers.
 */

import Foundation
import XCTest
@testable import RalphCore

@MainActor
final class WindowStateRestorationTests: WindowStateTestCase {
    func test_restoreWindows_withNoSavedState_createsDefault() {
        defaults.removeObject(forKey: testRestorationKey)

        let restored = manager.restoreWindows()
        XCTAssertEqual(restored.count, 1)
        XCTAssertEqual(restored.first?.workspaceIDs.count, 1)
    }

    func test_restoreWindows_withNoSavedState_usesExistingWorkspace() throws {
        defaults.removeObject(forKey: testRestorationKey)
        let temp = try makeSeededWorkspaceDirectory(prefix: "restore-windows-existing")
        defer { RalphCoreTestSupport.assertRemoved(temp) }

        let workspace = manager.createWorkspace(workingDirectory: temp)
        let restored = manager.restoreWindows()

        XCTAssertEqual(restored.count, 1)
        XCTAssertEqual(restored.first?.workspaceIDs, [workspace.id])
    }

    func test_restoreWindows_withValidSavedState_restoresCorrectly() throws {
        let directory1 = try makeSeededWorkspaceDirectory(prefix: "restore-valid-state-a")
        let directory2 = try makeSeededWorkspaceDirectory(prefix: "restore-valid-state-b")
        defer {
            RalphCoreTestSupport.assertRemoved(directory1)
            RalphCoreTestSupport.assertRemoved(directory2)
        }

        let workspace1 = manager.createWorkspace(workingDirectory: directory1)
        let workspace2 = manager.createWorkspace(workingDirectory: directory2)
        let state = WindowState(workspaceIDs: [workspace1.id, workspace2.id], selectedTabIndex: 1)
        manager.saveWindowState(state)

        let restored = manager.restoreWindows()

        XCTAssertEqual(restored.count, 1)
        XCTAssertEqual(restored.first?.id, state.id)
        XCTAssertEqual(restored.first?.workspaceIDs.count, 2)
    }

    func test_restoreWindows_doesNotBootstrapRestoredWorkspacesBeforeTheyAppear() async throws {
        let originalClient = manager.client
        defer { manager.client = originalClient }

        let fixture = try RalphMockCLITestSupport.makeFixture(
            prefix: "restore-no-eager-bootstrap",
            workspaceName: "workspace",
            logFileName: "workspace-overview.log",
            seedQueueTasks: []
        )
        defer { RalphCoreTestSupport.assertRemoved(fixture.rootURL) }

        let overviewURL = try WorkspaceRunnerConfigurationTestSupport.writeWorkspaceOverviewDocument(
            in: fixture.rootURL,
            name: "overview.json",
            workspaceURL: fixture.workspaceURL,
            activeTasks: [],
            model: "restored-model",
            phases: 2,
            iterations: 3
        )
        let script = """
            #!/bin/sh
            printf '%s\n' "$*" >> "\(fixture.logURL!.path)"
            case "$*" in
            *"--no-color machine workspace overview"*)
              cat "\(overviewURL.path)"
              exit 0
              ;;
            esac
            echo "unexpected args: $*" 1>&2
            exit 64
            """
        let scriptURL = try RalphMockCLITestSupport.makeExecutableScript(
            in: fixture.rootURL,
            name: "mock-ralph-restore-no-eager-bootstrap",
            body: script
        )
        manager.client = try RalphCLIClient(executableURL: scriptURL)

        let workspaceID = UUID()
        let snapshot = RalphWorkspaceDefaultsSnapshot(
            name: "workspace",
            workingDirectoryURL: fixture.workspaceURL,
            recentWorkingDirectories: []
        )
        let snapshotData = try JSONEncoder().encode(snapshot)
        defaults.set(snapshotData, forKey: workspaceSnapshotKey(for: workspaceID))

        let state = WindowState(workspaceIDs: [workspaceID], selectedTabIndex: 0)
        manager.saveWindowState(state)

        let restored = manager.restoreWindows()

        XCTAssertEqual(restored.first?.workspaceIDs, [workspaceID])
        let loggedCommands = (try? String(contentsOf: fixture.logURL!, encoding: .utf8)) ?? ""
        XCTAssertTrue(loggedCommands.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
        if let restoredWorkspace = manager.workspaces.first(where: { $0.id == workspaceID }) {
            manager.closeWorkspace(restoredWorkspace)
        }
    }

    func test_claimWindowState_returnsDistinctStates_forMultipleClaims() throws {
        let directory1 = try makeSeededWorkspaceDirectory(prefix: "claim-window-state-a")
        let directory2 = try makeSeededWorkspaceDirectory(prefix: "claim-window-state-b")
        defer {
            RalphCoreTestSupport.assertRemoved(directory1)
            RalphCoreTestSupport.assertRemoved(directory2)
        }

        let workspace1 = manager.createWorkspace(workingDirectory: directory1)
        let workspace2 = manager.createWorkspace(workingDirectory: directory2)
        let state1 = WindowState(workspaceIDs: [workspace1.id], selectedTabIndex: 0)
        let state2 = WindowState(workspaceIDs: [workspace2.id], selectedTabIndex: 0)
        manager.saveWindowState(state1)
        manager.saveWindowState(state2)

        let firstClaim = manager.claimWindowState(preferredID: nil)
        let secondClaim = manager.claimWindowState(preferredID: nil)

        XCTAssertNotEqual(firstClaim.id, secondClaim.id)
        XCTAssertTrue([state1.id, state2.id].contains(firstClaim.id))
        XCTAssertTrue([state1.id, state2.id].contains(secondClaim.id))
    }

    func test_claimWindowState_prefersProvidedID_whenAvailable() throws {
        let directory1 = try makeSeededWorkspaceDirectory(prefix: "claim-preferred-a")
        let directory2 = try makeSeededWorkspaceDirectory(prefix: "claim-preferred-b")
        defer {
            RalphCoreTestSupport.assertRemoved(directory1)
            RalphCoreTestSupport.assertRemoved(directory2)
        }

        let workspace1 = manager.createWorkspace(workingDirectory: directory1)
        let workspace2 = manager.createWorkspace(workingDirectory: directory2)
        let state1 = WindowState(workspaceIDs: [workspace1.id], selectedTabIndex: 0)
        let state2 = WindowState(workspaceIDs: [workspace2.id], selectedTabIndex: 0)
        manager.saveWindowState(state1)
        manager.saveWindowState(state2)

        let claimed = manager.claimWindowState(preferredID: state2.id)
        XCTAssertEqual(claimed.id, state2.id)
    }

    func test_claimWindowState_withSamePreferredIDTwice_returnsUniqueStates() throws {
        let directory1 = try makeSeededWorkspaceDirectory(prefix: "claim-same-preferred-a")
        let directory2 = try makeSeededWorkspaceDirectory(prefix: "claim-same-preferred-b")
        defer {
            RalphCoreTestSupport.assertRemoved(directory1)
            RalphCoreTestSupport.assertRemoved(directory2)
        }

        let workspace1 = manager.createWorkspace(workingDirectory: directory1)
        let workspace2 = manager.createWorkspace(workingDirectory: directory2)
        let state1 = WindowState(workspaceIDs: [workspace1.id], selectedTabIndex: 0)
        let state2 = WindowState(workspaceIDs: [workspace2.id], selectedTabIndex: 0)
        manager.saveWindowState(state1)
        manager.saveWindowState(state2)

        let firstClaim = manager.claimWindowState(preferredID: state1.id)
        let secondClaim = manager.claimWindowState(preferredID: state1.id)

        XCTAssertEqual(firstClaim.id, state1.id)
        XCTAssertNotEqual(secondClaim.id, state1.id)
    }

    func test_workspaceProjectDisplayName_prefersWorkingDirectoryLeafName() throws {
        let temp = try makeWorkspaceDirectory(prefix: "workspace-project-display-name")
        defer { RalphCoreTestSupport.assertRemoved(temp) }

        let workspace = Workspace(workingDirectoryURL: temp)
        workspace.identityState.name = "RalphMac"

        XCTAssertEqual(workspace.projectDisplayName, temp.lastPathComponent)
    }

    func test_workspaceMatchesWorkingDirectory_normalizesInputURLs() throws {
        let temp = try makeWorkspaceDirectory(prefix: "workspace-match-working-directory")
        defer { RalphCoreTestSupport.assertRemoved(temp) }

        let nestedPath = temp.appendingPathComponent("..", isDirectory: true)
            .appendingPathComponent(temp.lastPathComponent, isDirectory: true)
        let workspace = Workspace(workingDirectoryURL: nestedPath)

        XCTAssertTrue(workspace.matchesWorkingDirectory(temp))
        XCTAssertEqual(workspace.normalizedWorkingDirectoryURL.path, temp.path)
    }

    func test_isURLRoutingPlaceholderWorkspace_trueForExplicitStartupPlaceholderDuringInitialLoad() throws {
        let temp = try makeSeededWorkspaceDirectory(prefix: "url-routing-placeholder-empty")
        defer { RalphCoreTestSupport.assertRemoved(temp) }

        let workspace = Workspace(workingDirectoryURL: temp, launchDisposition: .startupPlaceholder)
        workspace.taskState.tasksLoading = true

        XCTAssertTrue(workspace.isURLRoutingPlaceholderWorkspace)
    }

    func test_isURLRoutingPlaceholderWorkspace_falseForRegularWorkspaceEvenWhenQueueIsEmpty() throws {
        let temp = try makeSeededWorkspaceDirectory(prefix: "url-routing-placeholder-populated")
        defer { RalphCoreTestSupport.assertRemoved(temp) }

        let workspace = Workspace(workingDirectoryURL: temp)
        workspace.taskState.tasks = []
        workspace.taskState.tasksLoading = false
        workspace.taskState.tasksErrorMessage = nil

        XCTAssertFalse(workspace.isURLRoutingPlaceholderWorkspace)
    }

    func test_setWorkingDirectory_consumesURLRoutingPlaceholderDisposition() throws {
        let initialDirectory = try makeSeededWorkspaceDirectory(prefix: "url-routing-placeholder-initial")
        let targetDirectory = try makeSeededWorkspaceDirectory(prefix: "url-routing-placeholder-target")
        defer {
            RalphCoreTestSupport.assertRemoved(initialDirectory)
            RalphCoreTestSupport.assertRemoved(targetDirectory)
        }

        let workspace = Workspace(workingDirectoryURL: initialDirectory, launchDisposition: .startupPlaceholder)
        XCTAssertTrue(workspace.isURLRoutingPlaceholderWorkspace)

        workspace.setWorkingDirectory(targetDirectory)

        XCTAssertFalse(workspace.isURLRoutingPlaceholderWorkspace)
        XCTAssertEqual(workspace.identityState.workingDirectoryURL.path, targetDirectory.path)
    }

    func test_scheduleInitialRepositoryBootstrapIfNeeded_bootstrapsRestoredWorkspaceOnDemand() async throws {
        var workspace: Workspace!
        let fixture = try RalphMockCLITestSupport.makeFixture(
            prefix: "workspace-on-demand-bootstrap",
            workspaceName: "workspace",
            logFileName: "workspace-overview.log",
            seedQueueTasks: []
        )
        defer { RalphCoreTestSupport.shutdownAndRemove(fixture.rootURL, workspace) }

        let task = RalphMockCLITestSupport.task(
            id: "RQ-RESTORE",
            status: .todo,
            title: "Restore Task",
            priority: .medium,
            createdAt: "2026-03-12T00:00:00Z",
            updatedAt: "2026-03-12T00:00:00Z"
        )
        let overviewURL = try WorkspaceRunnerConfigurationTestSupport.writeWorkspaceOverviewDocument(
            in: fixture.rootURL,
            name: "overview-on-demand.json",
            workspaceURL: fixture.workspaceURL,
            activeTasks: [task],
            nextRunnableTaskID: task.id,
            model: "on-demand-model",
            phases: 2,
            iterations: 3
        )
        let script = """
            #!/bin/sh
            printf '%s\n' "$*" >> "\(fixture.logURL!.path)"
            case "$*" in
            *"--no-color machine workspace overview"*)
              cat "\(overviewURL.path)"
              exit 0
              ;;
            esac
            echo "unexpected args: $*" 1>&2
            exit 64
            """
        let scriptURL = try RalphMockCLITestSupport.makeExecutableScript(
            in: fixture.rootURL,
            name: "mock-ralph-on-demand-bootstrap",
            body: script
        )
        let client = try RalphCLIClient(executableURL: scriptURL)

        workspace = Workspace(
            workingDirectoryURL: fixture.workspaceURL,
            client: client,
            bootstrapRepositoryStateOnInit: false
        )

        workspace.scheduleInitialRepositoryBootstrapIfNeeded()

        let bootstrapped = await WorkspacePerformanceTestSupport.waitFor(timeout: 3.0) {
            workspace.taskState.tasks.map(\.id) == ["RQ-RESTORE"]
                && workspace.runState.currentRunnerConfig?.model == "on-demand-model"
        }
        XCTAssertTrue(bootstrapped)
    }
}
