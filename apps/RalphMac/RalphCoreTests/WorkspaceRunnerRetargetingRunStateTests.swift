/**
 WorkspaceRunnerRetargetingRunStateTests

 Purpose:
 - Validate working-directory retargeting invalidates in-flight run state deterministically.

 Responsibilities:
 - Validate working-directory retargeting invalidates in-flight run state deterministically.

 Does not handle:
 - Runner-configuration decoding assertions.
 - Workspace-manager CLI override behavior.

 Usage:
 - Used by the RalphMac app or RalphCore tests through its owning feature surface.

 Invariants/assumptions callers must respect:
 - Mock run scripts cooperate with SIGINT/SIGTERM cancellation and expose only the exercised command surface.
 */

import Foundation
import XCTest

@testable import RalphCore

@MainActor
final class WorkspaceRunnerRetargetingRunStateTests: WorkspacePerformanceTestCase {
    func test_setWorkingDirectory_invalidatesActiveRunState() async throws {
        var workspace: Workspace!
        let rootDir = try WorkspacePerformanceTestSupport.makeTempDir(prefix: "ralph-workspace-retarget-run")
        defer { RalphCoreTestSupport.shutdownAndRemove(rootDir, workspace) }
        let workspaceADir = rootDir.appendingPathComponent("workspace-a", isDirectory: true)
        let workspaceBDir = rootDir.appendingPathComponent("workspace-b", isDirectory: true)
        try FileManager.default.createDirectory(at: workspaceADir, withIntermediateDirectories: true)
        try FileManager.default.createDirectory(at: workspaceBDir, withIntermediateDirectories: true)
        try RalphMockCLITestSupport.writeQueueFile(in: workspaceADir, tasks: [])
        try RalphMockCLITestSupport.writeQueueFile(in: workspaceBDir, tasks: [])

        let configAURL = try WorkspaceRunnerConfigurationTestSupport.writeConfigResolveDocument(
            in: rootDir,
            name: "config-a.json",
            workspaceURL: workspaceADir,
            model: "runner-model",
            phases: 1,
            iterations: 1
        )
        let configBURL = try WorkspaceRunnerConfigurationTestSupport.writeConfigResolveDocument(
            in: rootDir,
            name: "config-b.json",
            workspaceURL: workspaceBDir,
            model: "runner-model",
            phases: 1,
            iterations: 1
        )
        let specURL = try WorkspaceRunnerConfigurationTestSupport.writeCLISpecDocument(
            in: rootDir,
            name: "cli-spec.json",
            machineLeafName: nil
        )
        let queueAURL = try WorkspaceRunnerConfigurationTestSupport.writeQueueReadDocument(
            in: rootDir,
            name: "queue-a.json",
            workspaceURL: workspaceADir,
            activeTasks: []
        )
        let queueBURL = try WorkspaceRunnerConfigurationTestSupport.writeQueueReadDocument(
            in: rootDir,
            name: "queue-b.json",
            workspaceURL: workspaceBDir,
            activeTasks: []
        )
        let graphURL = try WorkspaceRunnerConfigurationTestSupport.writeGraphDocument(
            in: rootDir,
            name: "graph.json",
            tasks: [],
            runnableTasks: 0,
            blockedTasks: 0
        )

        let script = """
            #!/bin/sh
            trap 'exit 130' INT TERM

            if [ "$2" = "run" ] && [ "$3" = "one" ]; then
              echo "running-$PWD"
              sleep 5
              exit 0
            fi

            case "$PWD" in
            */workspace-a) workspace="a" ;;
            */workspace-b) workspace="b" ;;
            *) workspace="unknown" ;;
            esac

            if [ "$2" = "machine" ] && [ "$3" = "config" ] && [ "$4" = "resolve" ]; then
              if [ "$workspace" = "a" ]; then
                cat "\(configAURL.path)"
              else
                cat "\(configBURL.path)"
              fi
              exit 0
            fi

            if [ "$2" = "machine" ] && [ "$3" = "cli-spec" ]; then
              cat "\(specURL.path)"
              exit 0
            fi

            if [ "$2" = "machine" ] && [ "$3" = "queue" ] && [ "$4" = "read" ]; then
              if [ "$workspace" = "a" ]; then
                cat "\(queueAURL.path)"
              else
                cat "\(queueBURL.path)"
              fi
              exit 0
            fi

            if [ "$2" = "machine" ] && [ "$3" = "queue" ] && [ "$4" = "graph" ]; then
              cat "\(graphURL.path)"
              exit 0
            fi

            echo "unexpected args: $*" >&2
            exit 64
            """
        let scriptURL = try RalphMockCLITestSupport.makeExecutableScript(
            in: rootDir,
            name: "mock-ralph-retarget-run",
            body: script
        )
        let client = try RalphCLIClient(executableURL: scriptURL)
        workspace = Workspace(workingDirectoryURL: workspaceADir, client: client)

        workspace.run(arguments: ["--no-color", "run", "one"])

        let started = await WorkspacePerformanceTestSupport.waitFor(timeout: 2.0) {
            workspace.runState.isRunning && workspace.runState.output.contains("running-")
        }
        XCTAssertTrue(started)

        workspace.setWorkingDirectory(workspaceBDir)

        let cancelled = await WorkspacePerformanceTestSupport.waitFor(timeout: 3.0) {
            !workspace.runState.isRunning
                && workspace.runState.output.isEmpty
                && workspace.runState.currentTaskID == nil
        }
        XCTAssertTrue(cancelled)
        XCTAssertEqual(workspace.identityState.workingDirectoryURL, workspaceBDir)
        XCTAssertTrue(workspace.runState.executionHistory.isEmpty)
    }
}
