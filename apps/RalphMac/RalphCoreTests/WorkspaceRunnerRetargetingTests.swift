/**
 WorkspaceRunnerRetargetingTests

 Responsibilities:
 - Validate working-directory retargeting refreshes runner configuration and repository-derived workspace state.
 - Guard against stale async results bleeding across workspace generation changes.

 Does not handle:
 - Shutdown suppression coverage.
 - Workspace-manager CLI override behavior.

 Invariants/assumptions callers must respect:
 - Mock CLIs route behavior from `PWD` and only implement the machine surfaces exercised here.
 */

import Foundation
import XCTest

@testable import RalphCore

@MainActor
final class WorkspaceRunnerRetargetingTests: WorkspacePerformanceTestCase {
    func test_workspaceBootstrap_loadsTasksAndRunnerConfigurationWithoutGraphAnalyticsOrCLISpec() async throws {
        var workspace: Workspace!
        let rootDir = try WorkspacePerformanceTestSupport.makeTempDir(prefix: "ralph-workspace-bootstrap-minimal")
        defer { RalphCoreTestSupport.shutdownAndRemove(rootDir, workspace) }
        let workspaceDir = rootDir.appendingPathComponent("workspace", isDirectory: true)
        try FileManager.default.createDirectory(at: workspaceDir, withIntermediateDirectories: true)

        let task = RalphMockCLITestSupport.task(
            id: "RQ-BOOT",
            status: .todo,
            title: "Bootstrap Task",
            priority: .high,
            createdAt: "2026-03-05T00:00:00Z",
            updatedAt: "2026-03-05T00:00:00Z"
        )
        try RalphMockCLITestSupport.writeQueueFile(in: workspaceDir, tasks: [task])

        let overviewURL = try WorkspaceRunnerConfigurationTestSupport.writeWorkspaceOverviewDocument(
            in: rootDir,
            name: "overview-bootstrap.json",
            workspaceURL: workspaceDir,
            activeTasks: [task],
            nextRunnableTaskID: "RQ-BOOT",
            model: "bootstrap-model",
            phases: 2,
            iterations: 3
        )

        let script = """
            #!/bin/sh
            case "$*" in
            *"--no-color machine workspace overview"*)
              cat "\(overviewURL.path)"
              exit 0
              ;;

            *"--no-color machine queue read"*|*"--no-color machine config resolve"*)
              echo "unexpected legacy bootstrap command: $*" 1>&2
              exit 64
              ;;

            *"--no-color machine queue graph"*|*"--no-color machine cli-spec"*|*"stats"*|*"report"*)
              echo "unexpected bootstrap command: $*" 1>&2
              exit 64
              ;;
            esac

            echo "unexpected args: $*" 1>&2
            exit 64
            """
        let scriptURL = try RalphMockCLITestSupport.makeExecutableScript(
            in: rootDir,
            name: "mock-ralph-bootstrap-minimal",
            body: script
        )
        let client = try RalphCLIClient(executableURL: scriptURL)
        workspace = Workspace(workingDirectoryURL: workspaceDir, client: client)

        let bootstrapped = await WorkspacePerformanceTestSupport.waitFor(timeout: 3.0) {
            workspace.taskState.tasks.map(\.id) == ["RQ-BOOT"]
                && workspace.runState.currentRunnerConfig?.model == "bootstrap-model"
        }
        XCTAssertTrue(bootstrapped)
        XCTAssertNil(workspace.insightsState.graphData)
        XCTAssertNil(workspace.commandState.cliSpec)
        XCTAssertNil(workspace.insightsState.analytics.lastRefreshedAt)
    }

    func test_setWorkingDirectory_refreshesRunnerConfiguration() async throws {
        var workspace: Workspace!
        let rootDir = try WorkspacePerformanceTestSupport.makeTempDir(prefix: "ralph-workspace-config-switch")
        defer { RalphCoreTestSupport.shutdownAndRemove(rootDir, workspace) }
        let workspaceADir = rootDir.appendingPathComponent("workspace-a", isDirectory: true)
        let workspaceBDir = rootDir.appendingPathComponent("workspace-b", isDirectory: true)
        try FileManager.default.createDirectory(at: workspaceADir, withIntermediateDirectories: true)
        try FileManager.default.createDirectory(at: workspaceBDir, withIntermediateDirectories: true)

        let configAURL = try WorkspaceRunnerConfigurationTestSupport.writeConfigResolveDocument(
            in: rootDir,
            name: "config-a.json",
            workspaceURL: workspaceADir,
            model: "model-a",
            phases: 1,
            iterations: 1
        )
        let configBURL = try WorkspaceRunnerConfigurationTestSupport.writeConfigResolveDocument(
            in: rootDir,
            name: "config-b.json",
            workspaceURL: workspaceBDir,
            model: "model-b",
            phases: 2,
            iterations: 4
        )
        let configUnknownURL = try WorkspaceRunnerConfigurationTestSupport.writeConfigResolveDocument(
            in: rootDir,
            name: "config-unknown.json",
            workspaceURL: rootDir,
            model: "model-unknown",
            phases: 3,
            iterations: 9
        )

        let switchScript = """
            #!/bin/sh
            if [ "$2" = "machine" ] && [ "$3" = "config" ] && [ "$4" = "resolve" ]; then
              case "$PWD" in
              */workspace-a)
                cat "\(configAURL.path)"
                ;;
              */workspace-b)
                cat "\(configBURL.path)"
                ;;
              *)
                cat "\(configUnknownURL.path)"
                ;;
              esac
              exit 0
            fi
            exit 64
            """
        let scriptURL = try RalphMockCLITestSupport.makeExecutableScript(
            in: rootDir,
            name: "mock-ralph-switch",
            body: switchScript
        )
        let client = try RalphCLIClient(executableURL: scriptURL)
        workspace = Workspace(workingDirectoryURL: workspaceADir, client: client)

        await workspace.loadRunnerConfiguration(retryConfiguration: .minimal)
        XCTAssertEqual(workspace.runState.currentRunnerConfig?.model, "model-a")
        XCTAssertEqual(workspace.runState.currentRunnerConfig?.phases, 1)
        XCTAssertEqual(workspace.runState.currentRunnerConfig?.maxIterations, 1)

        workspace.setWorkingDirectory(workspaceBDir)

        let switchedRunnerConfig = await WorkspacePerformanceTestSupport.waitFor(timeout: 2.0) {
            workspace.runState.currentRunnerConfig?.model == "model-b"
                && workspace.runState.currentRunnerConfig?.phases == 2
                && workspace.runState.currentRunnerConfig?.maxIterations == 4
        }
        XCTAssertTrue(switchedRunnerConfig)
        XCTAssertEqual(workspace.runState.currentRunnerConfig?.model, "model-b")
        XCTAssertEqual(workspace.runState.currentRunnerConfig?.phases, 2)
        XCTAssertEqual(workspace.runState.currentRunnerConfig?.maxIterations, 4)
    }

    func test_setWorkingDirectory_clearsRepositoryDerivedStateImmediately_andReloadsNewRepository() async throws {
        var workspace: Workspace!
        let rootDir = try WorkspacePerformanceTestSupport.makeTempDir(prefix: "ralph-workspace-retarget")
        defer { RalphCoreTestSupport.shutdownAndRemove(rootDir, workspace) }
        let workspaceADir = rootDir.appendingPathComponent("workspace-a", isDirectory: true)
        let workspaceBDir = rootDir.appendingPathComponent("workspace-b", isDirectory: true)
        try FileManager.default.createDirectory(at: workspaceADir, withIntermediateDirectories: true)
        try FileManager.default.createDirectory(at: workspaceBDir, withIntermediateDirectories: true)

        let workspaceATask = RalphMockCLITestSupport.task(
            id: "RQ-A",
            status: .todo,
            title: "Workspace A Task",
            priority: .high,
            createdAt: "2026-03-05T00:00:00Z",
            updatedAt: "2026-03-05T00:00:00Z"
        )
        let workspaceBTask = RalphMockCLITestSupport.task(
            id: "RQ-B",
            status: .todo,
            title: "Workspace B Task",
            priority: .medium,
            createdAt: "2026-03-06T00:00:00Z",
            updatedAt: "2026-03-06T00:00:00Z"
        )
        try RalphMockCLITestSupport.writeQueueFile(in: workspaceADir, tasks: [workspaceATask])
        try RalphMockCLITestSupport.writeQueueFile(in: workspaceBDir, tasks: [workspaceBTask])

        let queueAURL = try WorkspaceRunnerConfigurationTestSupport.writeQueueReadDocument(
            in: rootDir,
            name: "queue-a.json",
            workspaceURL: workspaceADir,
            activeTasks: [workspaceATask],
            nextRunnableTaskID: "RQ-A"
        )
        let queueBURL = try WorkspaceRunnerConfigurationTestSupport.writeQueueReadDocument(
            in: rootDir,
            name: "queue-b.json",
            workspaceURL: workspaceBDir,
            activeTasks: [workspaceBTask],
            nextRunnableTaskID: "RQ-B"
        )
        let graphAURL = try WorkspaceRunnerConfigurationTestSupport.writeGraphDocument(
            in: rootDir,
            name: "graph-a.json",
            tasks: [RalphMockCLITestSupport.graphNode(id: "RQ-A", title: "Graph A")]
        )
        let graphBURL = try WorkspaceRunnerConfigurationTestSupport.writeGraphDocument(
            in: rootDir,
            name: "graph-b.json",
            tasks: [RalphMockCLITestSupport.graphNode(id: "RQ-B", title: "Graph B")]
        )
        let specAURL = try WorkspaceRunnerConfigurationTestSupport.writeCLISpecDocument(
            in: rootDir,
            name: "cli-spec-a.json",
            machineLeafName: "task-a",
            about: "A"
        )
        let specBURL = try WorkspaceRunnerConfigurationTestSupport.writeCLISpecDocument(
            in: rootDir,
            name: "cli-spec-b.json",
            machineLeafName: "task-b",
            about: "B"
        )
        let configAURL = try WorkspaceRunnerConfigurationTestSupport.writeConfigResolveDocument(
            in: rootDir,
            name: "config-a.json",
            workspaceURL: workspaceADir,
            model: "model-a",
            phases: 1,
            iterations: 1
        )
        let configBURL = try WorkspaceRunnerConfigurationTestSupport.writeConfigResolveDocument(
            in: rootDir,
            name: "config-b.json",
            workspaceURL: workspaceBDir,
            model: "model-b",
            phases: 2,
            iterations: 4
        )

        let script = """
            #!/bin/sh
            case "$PWD" in
            */workspace-a) workspace="a" ;;
            */workspace-b) workspace="b" ;;
            *) workspace="unknown" ;;
            esac

            if [ "$workspace" = "b" ]; then
              sleep 0.3
            fi

            case "$*" in
            *"--no-color machine queue read"*)
              if [ "$workspace" = "a" ]; then
                cat "\(queueAURL.path)"
              else
                cat "\(queueBURL.path)"
              fi
              exit 0
              ;;

            *"--no-color machine queue graph"*)
              if [ "$workspace" = "a" ]; then
                cat "\(graphAURL.path)"
              else
                cat "\(graphBURL.path)"
              fi
              exit 0
              ;;

            *"--no-color machine cli-spec"*)
              if [ "$workspace" = "a" ]; then
                cat "\(specAURL.path)"
              else
                cat "\(specBURL.path)"
              fi
              exit 0
              ;;

            *"--no-color machine config resolve"*)
              if [ "$workspace" = "a" ]; then
                cat "\(configAURL.path)"
              else
                cat "\(configBURL.path)"
              fi
              exit 0
              ;;
            esac

            echo "unexpected args: $*" 1>&2
            exit 64
            """
        let scriptURL = try RalphMockCLITestSupport.makeExecutableScript(
            in: rootDir,
            name: "mock-ralph-retarget",
            body: script
        )
        let client = try RalphCLIClient(executableURL: scriptURL)
        workspace = Workspace(workingDirectoryURL: workspaceADir, client: client)

        await workspace.loadTasks(retryConfiguration: .minimal)
        await workspace.loadGraphData(retryConfiguration: .minimal)
        await workspace.loadCLISpec(retryConfiguration: .minimal)
        await workspace.loadRunnerConfiguration(retryConfiguration: .minimal)

        XCTAssertEqual(workspace.taskState.tasks.map(\.id), ["RQ-A"])
        XCTAssertEqual(workspace.insightsState.graphData?.tasks.map(\.id), ["RQ-A"])
        XCTAssertEqual(workspace.commandState.cliSpec?.root.subcommands.first?.subcommands.first?.name, "task-a")
        XCTAssertEqual(workspace.runState.currentRunnerConfig?.model, "model-a")

        workspace.setWorkingDirectory(workspaceBDir)

        XCTAssertEqual(workspace.identityState.workingDirectoryURL, workspaceBDir)
        XCTAssertTrue(workspace.taskState.tasks.isEmpty)
        XCTAssertNil(workspace.insightsState.graphData)
        XCTAssertNil(workspace.commandState.cliSpec)
        XCTAssertNil(workspace.runState.currentRunnerConfig)
        XCTAssertTrue(workspace.runState.output.isEmpty)
        XCTAssertTrue(workspace.runState.executionHistory.isEmpty)

        let reloaded = await WorkspacePerformanceTestSupport.waitFor(timeout: 3.0) {
            workspace.taskState.tasks.map(\.id) == ["RQ-B"]
                && workspace.runState.currentRunnerConfig?.model == "model-b"
        }
        XCTAssertTrue(reloaded)
        XCTAssertNil(workspace.insightsState.graphData)
        XCTAssertNil(workspace.commandState.cliSpec)
    }

    func test_repositoryGeneration_discardsLateResultsFromPreviousWorkspace() async throws {
        var workspace: Workspace!
        let rootDir = try WorkspacePerformanceTestSupport.makeTempDir(prefix: "ralph-workspace-retarget-stale")
        defer { RalphCoreTestSupport.shutdownAndRemove(rootDir, workspace) }
        let workspaceADir = rootDir.appendingPathComponent("workspace-a", isDirectory: true)
        let workspaceBDir = rootDir.appendingPathComponent("workspace-b", isDirectory: true)
        try FileManager.default.createDirectory(at: workspaceADir, withIntermediateDirectories: true)
        try FileManager.default.createDirectory(at: workspaceBDir, withIntermediateDirectories: true)

        let staleTask = RalphMockCLITestSupport.task(
            id: "RQ-A",
            status: .todo,
            title: "Stale A Task",
            priority: .high,
            createdAt: "2026-03-05T00:00:00Z",
            updatedAt: "2026-03-05T00:00:00Z"
        )
        let freshTask = RalphMockCLITestSupport.task(
            id: "RQ-B",
            status: .todo,
            title: "Fresh B Task",
            priority: .medium,
            createdAt: "2026-03-06T00:00:00Z",
            updatedAt: "2026-03-06T00:00:00Z"
        )
        try RalphMockCLITestSupport.writeQueueFile(in: workspaceADir, tasks: [staleTask])
        try RalphMockCLITestSupport.writeQueueFile(in: workspaceBDir, tasks: [freshTask])

        let queueAURL = try WorkspaceRunnerConfigurationTestSupport.writeQueueReadDocument(
            in: rootDir,
            name: "queue-a.json",
            workspaceURL: workspaceADir,
            activeTasks: [staleTask],
            nextRunnableTaskID: "RQ-A"
        )
        let queueBURL = try WorkspaceRunnerConfigurationTestSupport.writeQueueReadDocument(
            in: rootDir,
            name: "queue-b.json",
            workspaceURL: workspaceBDir,
            activeTasks: [freshTask],
            nextRunnableTaskID: "RQ-B"
        )
        let graphAURL = try WorkspaceRunnerConfigurationTestSupport.writeGraphDocument(
            in: rootDir,
            name: "graph-a.json",
            tasks: [RalphMockCLITestSupport.graphNode(id: "RQ-A", title: "Stale Graph A")]
        )
        let graphBURL = try WorkspaceRunnerConfigurationTestSupport.writeGraphDocument(
            in: rootDir,
            name: "graph-b.json",
            tasks: [RalphMockCLITestSupport.graphNode(id: "RQ-B", title: "Fresh Graph B")]
        )
        let specAURL = try WorkspaceRunnerConfigurationTestSupport.writeCLISpecDocument(
            in: rootDir,
            name: "cli-spec-a.json",
            machineLeafName: "stale-a",
            about: "A"
        )
        let specBURL = try WorkspaceRunnerConfigurationTestSupport.writeCLISpecDocument(
            in: rootDir,
            name: "cli-spec-b.json",
            machineLeafName: "fresh-b",
            about: "B"
        )
        let configAURL = try WorkspaceRunnerConfigurationTestSupport.writeConfigResolveDocument(
            in: rootDir,
            name: "config-a.json",
            workspaceURL: workspaceADir,
            model: "model-a-stale",
            phases: 1,
            iterations: 1
        )
        let configBURL = try WorkspaceRunnerConfigurationTestSupport.writeConfigResolveDocument(
            in: rootDir,
            name: "config-b.json",
            workspaceURL: workspaceBDir,
            model: "model-b-fresh",
            phases: 2,
            iterations: 2
        )

        let script = """
            #!/bin/sh
            case "$PWD" in
            */workspace-a) workspace="a" ;;
            */workspace-b) workspace="b" ;;
            *) workspace="unknown" ;;
            esac

            if [ "$workspace" = "a" ]; then
              sleep 0.5
            fi

            case "$*" in
            *"--no-color machine queue read"*)
              if [ "$workspace" = "a" ]; then
                cat "\(queueAURL.path)"
              else
                cat "\(queueBURL.path)"
              fi
              exit 0
              ;;

            *"--no-color machine queue graph"*)
              if [ "$workspace" = "a" ]; then
                cat "\(graphAURL.path)"
              else
                cat "\(graphBURL.path)"
              fi
              exit 0
              ;;

            *"--no-color machine cli-spec"*)
              if [ "$workspace" = "a" ]; then
                cat "\(specAURL.path)"
              else
                cat "\(specBURL.path)"
              fi
              exit 0
              ;;

            *"--no-color machine config resolve"*)
              if [ "$workspace" = "a" ]; then
                cat "\(configAURL.path)"
              else
                cat "\(configBURL.path)"
              fi
              exit 0
              ;;
            esac

            echo "unexpected args: $*" 1>&2
            exit 64
            """
        let scriptURL = try RalphMockCLITestSupport.makeExecutableScript(
            in: rootDir,
            name: "mock-ralph-retarget-stale",
            body: script
        )
        let client = try RalphCLIClient(executableURL: scriptURL)
        workspace = Workspace(workingDirectoryURL: workspaceADir, client: client)

        let staleTaskLoad = Task { @MainActor in
            await workspace.loadTasks(retryConfiguration: .minimal)
        }
        let staleGraphLoad = Task { @MainActor in
            await workspace.loadGraphData(retryConfiguration: .minimal)
        }
        let staleSpecLoad = Task { @MainActor in
            await workspace.loadCLISpec(retryConfiguration: .minimal)
        }
        let staleConfigLoad = Task { @MainActor in
            await workspace.loadRunnerConfiguration(retryConfiguration: .minimal)
        }

        workspace.setWorkingDirectory(workspaceBDir)

        let loadedFreshWorkspace = await WorkspacePerformanceTestSupport.waitFor(timeout: 3.0) {
            workspace.taskState.tasks.map(\.id) == ["RQ-B"]
                && workspace.insightsState.graphData?.tasks.map(\.id) == ["RQ-B"]
                && workspace.commandState.cliSpec?.root.subcommands.first?.subcommands.first?.name == "fresh-b"
                && workspace.runState.currentRunnerConfig?.model == "model-b-fresh"
        }
        XCTAssertTrue(loadedFreshWorkspace)

        _ = await staleTaskLoad.result
        _ = await staleGraphLoad.result
        _ = await staleSpecLoad.result
        _ = await staleConfigLoad.result

        XCTAssertEqual(workspace.taskState.tasks.map(\.id), ["RQ-B"])
        XCTAssertEqual(workspace.insightsState.graphData?.tasks.map(\.id), ["RQ-B"])
        XCTAssertEqual(workspace.commandState.cliSpec?.root.subcommands.first?.subcommands.first?.name, "fresh-b")
        XCTAssertEqual(workspace.runState.currentRunnerConfig?.model, "model-b-fresh")
    }
}
