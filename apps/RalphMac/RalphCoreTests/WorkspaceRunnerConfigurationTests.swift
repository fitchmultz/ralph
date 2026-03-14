/**
 WorkspaceRunnerConfigurationTests

 Responsibilities:
 - Validate runner-configuration loading, refresh, and workspace-manager CLI override behavior.

 Does not handle:
 - Run-control streaming or task-mutation payload generation.

 Invariants/assumptions callers must respect:
 - Mock CLIs emulate only the specific argument surfaces asserted by each test.
 */

import XCTest
@testable import RalphCore

@MainActor
final class WorkspaceRunnerConfigurationTests: WorkspacePerformanceTestCase {
    func test_loadRunnerConfiguration_setsCurrentRunnerConfig() async throws {
        var workspace: Workspace!
        let fixture = try RalphMockCLITestSupport.makeFixture(prefix: "ralph-workspace-config")
        defer { RalphCoreTestSupport.shutdownAndRemove(fixture.rootURL, workspace) }

        let configResolveURL = try Self.writeConfigResolveDocument(
            in: fixture.rootURL,
            name: "config-resolve.json",
            workspaceURL: fixture.workspaceURL,
            model: "kimi-code/kimi-for-coding",
            phases: 2,
            iterations: 3
        )

        let script = """
            #!/bin/sh
            if [ "$1" = "--no-color" ] && [ "$2" = "machine" ] && [ "$3" = "config" ] && [ "$4" = "resolve" ]; then
              cat "\(configResolveURL.path)"
              exit 0
            fi
            echo "unexpected args: $*" 1>&2
            exit 64
            """
        let scriptURL = try RalphMockCLITestSupport.makeExecutableScript(in: fixture.rootURL, body: script)
        let client = try RalphCLIClient(executableURL: scriptURL)
        workspace = Workspace(workingDirectoryURL: fixture.workspaceURL, client: client)

        await workspace.loadRunnerConfiguration(retryConfiguration: .minimal)

        XCTAssertEqual(workspace.runState.currentRunnerConfig?.model, "kimi-code/kimi-for-coding")
        XCTAssertEqual(workspace.runState.currentRunnerConfig?.phases, 2)
        XCTAssertEqual(workspace.runState.currentRunnerConfig?.maxIterations, 3)
    }

    func test_loadRunnerConfiguration_decodesSafetySummary() async throws {
        var workspace: Workspace!
        let fixture = try RalphMockCLITestSupport.makeFixture(prefix: "ralph-workspace-config-safety")
        defer { RalphCoreTestSupport.shutdownAndRemove(fixture.rootURL, workspace) }

        let safety = MachineConfigSafetySummary(
            repoTrusted: false,
            dirtyRepo: true,
            gitPublishMode: "commit_and_push",
            approvalMode: "yolo",
            ciGateEnabled: false,
            gitRevertMode: "disabled",
            parallelConfigured: true,
            executionInteractivity: "noninteractive_streaming",
            interactiveApprovalSupported: false
        )
        let configResolveURL = try Self.writeConfigResolveDocument(
            in: fixture.rootURL,
            name: "config-resolve.json",
            workspaceURL: fixture.workspaceURL,
            model: "gpt-5.4",
            phases: 3,
            iterations: 2,
            gitPublishMode: "commit_and_push",
            safety: safety
        )

        let script = """
            #!/bin/sh
            if [ "$1" = "--no-color" ] && [ "$2" = "machine" ] && [ "$3" = "config" ] && [ "$4" = "resolve" ]; then
              cat "\(configResolveURL.path)"
              exit 0
            fi
            echo "unexpected args: $*" 1>&2
            exit 64
            """
        let scriptURL = try RalphMockCLITestSupport.makeExecutableScript(in: fixture.rootURL, name: "mock-ralph-safety", body: script)
        let client = try RalphCLIClient(executableURL: scriptURL)
        workspace = Workspace(workingDirectoryURL: fixture.workspaceURL, client: client)

        await workspace.loadRunnerConfiguration(retryConfiguration: .minimal)

        XCTAssertEqual(workspace.runState.currentRunnerConfig?.model, "gpt-5.4")
        XCTAssertEqual(workspace.runState.currentRunnerConfig?.phases, 3)
        XCTAssertEqual(workspace.runState.currentRunnerConfig?.maxIterations, 2)
        XCTAssertEqual(workspace.runState.currentRunnerConfig?.safety?.repoTrusted, false)
        XCTAssertEqual(workspace.runState.currentRunnerConfig?.safety?.dirtyRepo, true)
        XCTAssertEqual(workspace.runState.currentRunnerConfig?.safety?.gitPublishMode, "commit_and_push")
        XCTAssertEqual(workspace.runState.currentRunnerConfig?.safety?.approvalMode, "yolo")
        XCTAssertEqual(workspace.runState.currentRunnerConfig?.safety?.ciGateEnabled, false)
        XCTAssertEqual(workspace.runState.currentRunnerConfig?.safety?.gitRevertMode, "disabled")
        XCTAssertEqual(workspace.runState.currentRunnerConfig?.safety?.parallelConfigured, true)
        XCTAssertEqual(workspace.runState.currentRunnerConfig?.safety?.executionInteractivity, "noninteractive_streaming")
        XCTAssertEqual(workspace.runState.currentRunnerConfig?.safety?.interactiveApprovalSupported, false)
    }

    func test_loadRunnerConfiguration_onFailure_clearsCurrentRunnerConfig() async throws {
        var workspace: Workspace!
        let fixture = try RalphMockCLITestSupport.makeFixture(prefix: "ralph-workspace-config-failure")
        defer { RalphCoreTestSupport.shutdownAndRemove(fixture.rootURL, workspace) }

        let successConfigURL = try Self.writeConfigResolveDocument(
            in: fixture.rootURL,
            name: "config-success.json",
            workspaceURL: fixture.workspaceURL,
            model: "kimi-initial",
            phases: 3,
            iterations: 2
        )

        let successScript = """
            #!/bin/sh
            if [ "$2" = "machine" ] && [ "$3" = "config" ] && [ "$4" = "resolve" ]; then
              cat "\(successConfigURL.path)"
              exit 0
            fi
            exit 64
            """
        let successScriptURL = try RalphMockCLITestSupport.makeExecutableScript(
            in: fixture.rootURL,
            name: "mock-ralph-success",
            body: successScript
        )
        let successClient = try RalphCLIClient(executableURL: successScriptURL)
        workspace = Workspace(workingDirectoryURL: fixture.workspaceURL, client: successClient)
                await workspace.loadRunnerConfiguration(retryConfiguration: .minimal)
        XCTAssertEqual(workspace.runState.currentRunnerConfig?.model, "kimi-initial")
        XCTAssertEqual(workspace.runState.currentRunnerConfig?.phases, 3)
        XCTAssertEqual(workspace.runState.currentRunnerConfig?.maxIterations, 2)

        let failScript = """
            #!/bin/sh
            echo "config failed" 1>&2
            exit 1
            """
        let failScriptURL = try RalphMockCLITestSupport.makeExecutableScript(
            in: fixture.rootURL,
            name: "mock-ralph-fail",
            body: failScript
        )
        let failClient = try RalphCLIClient(executableURL: failScriptURL)
        workspace.injectClient(failClient)

        let clearedRunnerConfig = await WorkspacePerformanceTestSupport.waitFor(timeout: 2.0) {
            workspace.runState.currentRunnerConfig == nil
        }
        XCTAssertTrue(clearedRunnerConfig)

        XCTAssertNil(workspace.runState.currentRunnerConfig)
    }

    func test_shutdown_prevents_runnerConfiguration_reload_activity() async throws {
        var workspace: Workspace!
        let fixture = try RalphMockCLITestSupport.makeFixture(prefix: "ralph-workspace-config-shutdown")
        defer { RalphCoreTestSupport.shutdownAndRemove(fixture.rootURL, workspace) }

        let logURL = fixture.rootURL.appendingPathComponent("config-resolve.log", isDirectory: false)
        let configResolveURL = try Self.writeConfigResolveDocument(
            in: fixture.rootURL,
            name: "config-resolve.json",
            workspaceURL: fixture.workspaceURL,
            model: "should-not-load",
            phases: 1,
            iterations: 1
        )

        let script = """
            #!/bin/sh
            printf '%s\n' "$*" >> "\(logURL.path)"
            if [ "$1" = "--no-color" ] && [ "$2" = "machine" ] && [ "$3" = "config" ] && [ "$4" = "resolve" ]; then
              cat "\(configResolveURL.path)"
              exit 0
            fi
            echo "unexpected args: $*" 1>&2
            exit 64
            """
        let scriptURL = try RalphMockCLITestSupport.makeExecutableScript(
            in: fixture.rootURL,
            name: "mock-ralph-shutdown",
            body: script
        )
        let client = try RalphCLIClient(executableURL: scriptURL)
        workspace = Workspace(workingDirectoryURL: fixture.workspaceURL)
                workspace.client = client

        workspace.shutdown()
        await workspace.loadRunnerConfiguration(retryConfiguration: .minimal)

        XCTAssertFalse(FileManager.default.fileExists(atPath: logURL.path))
        XCTAssertNil(workspace.runState.currentRunnerConfig)
    }

    func test_setWorkingDirectory_refreshesRunnerConfiguration() async throws {
        var workspace: Workspace!
        let rootDir = try WorkspacePerformanceTestSupport.makeTempDir(prefix: "ralph-workspace-config-switch")
        defer { RalphCoreTestSupport.shutdownAndRemove(rootDir, workspace) }
        let workspaceADir = rootDir.appendingPathComponent("workspace-a", isDirectory: true)
        let workspaceBDir = rootDir.appendingPathComponent("workspace-b", isDirectory: true)
        try FileManager.default.createDirectory(at: workspaceADir, withIntermediateDirectories: true)
        try FileManager.default.createDirectory(at: workspaceBDir, withIntermediateDirectories: true)

        let configAURL = try Self.writeConfigResolveDocument(
            in: rootDir,
            name: "config-a.json",
            workspaceURL: workspaceADir,
            model: "model-a",
            phases: 1,
            iterations: 1
        )
        let configBURL = try Self.writeConfigResolveDocument(
            in: rootDir,
            name: "config-b.json",
            workspaceURL: workspaceBDir,
            model: "model-b",
            phases: 2,
            iterations: 4
        )
        let configUnknownURL = try Self.writeConfigResolveDocument(
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

        let queueAURL = try Self.writeQueueReadDocument(
            in: rootDir,
            name: "queue-a.json",
            workspaceURL: workspaceADir,
            activeTasks: [workspaceATask],
            nextRunnableTaskID: "RQ-A"
        )
        let queueBURL = try Self.writeQueueReadDocument(
            in: rootDir,
            name: "queue-b.json",
            workspaceURL: workspaceBDir,
            activeTasks: [workspaceBTask],
            nextRunnableTaskID: "RQ-B"
        )
        let graphAURL = try Self.writeGraphDocument(
            in: rootDir,
            name: "graph-a.json",
            tasks: [RalphMockCLITestSupport.graphNode(id: "RQ-A", title: "Graph A")]
        )
        let graphBURL = try Self.writeGraphDocument(
            in: rootDir,
            name: "graph-b.json",
            tasks: [RalphMockCLITestSupport.graphNode(id: "RQ-B", title: "Graph B")]
        )
        let specAURL = try Self.writeCLISpecDocument(in: rootDir, name: "cli-spec-a.json", machineLeafName: "task-a", about: "A")
        let specBURL = try Self.writeCLISpecDocument(in: rootDir, name: "cli-spec-b.json", machineLeafName: "task-b", about: "B")
        let configAURL = try Self.writeConfigResolveDocument(
            in: rootDir,
            name: "config-a.json",
            workspaceURL: workspaceADir,
            model: "model-a",
            phases: 1,
            iterations: 1
        )
        let configBURL = try Self.writeConfigResolveDocument(
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
                && workspace.insightsState.graphData?.tasks.map(\.id) == ["RQ-B"]
                && workspace.commandState.cliSpec?.root.subcommands.first?.subcommands.first?.name == "task-b"
                && workspace.runState.currentRunnerConfig?.model == "model-b"
        }
        XCTAssertTrue(reloaded)
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

        let queueAURL = try Self.writeQueueReadDocument(
            in: rootDir,
            name: "queue-a.json",
            workspaceURL: workspaceADir,
            activeTasks: [staleTask],
            nextRunnableTaskID: "RQ-A"
        )
        let queueBURL = try Self.writeQueueReadDocument(
            in: rootDir,
            name: "queue-b.json",
            workspaceURL: workspaceBDir,
            activeTasks: [freshTask],
            nextRunnableTaskID: "RQ-B"
        )
        let graphAURL = try Self.writeGraphDocument(
            in: rootDir,
            name: "graph-a.json",
            tasks: [RalphMockCLITestSupport.graphNode(id: "RQ-A", title: "Stale Graph A")]
        )
        let graphBURL = try Self.writeGraphDocument(
            in: rootDir,
            name: "graph-b.json",
            tasks: [RalphMockCLITestSupport.graphNode(id: "RQ-B", title: "Fresh Graph B")]
        )
        let specAURL = try Self.writeCLISpecDocument(in: rootDir, name: "cli-spec-a.json", machineLeafName: "stale-a", about: "A")
        let specBURL = try Self.writeCLISpecDocument(in: rootDir, name: "cli-spec-b.json", machineLeafName: "fresh-b", about: "B")
        let configAURL = try Self.writeConfigResolveDocument(
            in: rootDir,
            name: "config-a.json",
            workspaceURL: workspaceADir,
            model: "model-a-stale",
            phases: 1,
            iterations: 1
        )
        let configBURL = try Self.writeConfigResolveDocument(
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

        let configAURL = try Self.writeConfigResolveDocument(
            in: rootDir,
            name: "config-a.json",
            workspaceURL: workspaceADir,
            model: "runner-model",
            phases: 1,
            iterations: 1
        )
        let configBURL = try Self.writeConfigResolveDocument(
            in: rootDir,
            name: "config-b.json",
            workspaceURL: workspaceBDir,
            model: "runner-model",
            phases: 1,
            iterations: 1
        )
        let specURL = try Self.writeCLISpecDocument(in: rootDir, name: "cli-spec.json", machineLeafName: nil)
        let queueAURL = try Self.writeQueueReadDocument(in: rootDir, name: "queue-a.json", workspaceURL: workspaceADir, activeTasks: [])
        let queueBURL = try Self.writeQueueReadDocument(in: rootDir, name: "queue-b.json", workspaceURL: workspaceBDir, activeTasks: [])
        let graphURL = try Self.writeGraphDocument(in: rootDir, name: "graph.json", tasks: [], runnableTasks: 0, blockedTasks: 0)

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

    private static func writeConfigResolveDocument(
        in directory: URL,
        name: String,
        workspaceURL: URL,
        model: String,
        phases: Int? = nil,
        iterations: Int? = nil,
        gitPublishMode: String? = nil,
        safety: MachineConfigSafetySummary = RalphMockCLITestSupport.defaultSafetySummary
    ) throws -> URL {
        try RalphMockCLITestSupport.writeJSONDocument(
            RalphMockCLITestSupport.configResolveDocument(
                workspaceURL: workspaceURL,
                safety: safety,
                agent: AgentConfig(
                    model: model,
                    phases: phases,
                    iterations: iterations,
                    gitPublishMode: gitPublishMode
                )
            ),
            in: directory,
            name: name
        )
    }

    private static func writeQueueReadDocument(
        in directory: URL,
        name: String,
        workspaceURL: URL,
        activeTasks: [RalphTask],
        doneTasks: [RalphTask] = [],
        nextRunnableTaskID: String? = nil
    ) throws -> URL {
        try RalphMockCLITestSupport.writeJSONDocument(
            RalphMockCLITestSupport.queueReadDocument(
                workspaceURL: workspaceURL,
                activeTasks: activeTasks,
                doneTasks: doneTasks,
                nextRunnableTaskID: nextRunnableTaskID
            ),
            in: directory,
            name: name
        )
    }

    private static func writeGraphDocument(
        in directory: URL,
        name: String,
        tasks: [RalphGraphNode],
        runnableTasks: Int? = nil,
        blockedTasks: Int = 0
    ) throws -> URL {
        try RalphMockCLITestSupport.writeJSONDocument(
            RalphMockCLITestSupport.graphReadDocument(
                tasks: tasks,
                runnableTasks: runnableTasks,
                blockedTasks: blockedTasks
            ),
            in: directory,
            name: name
        )
    }

    private static func writeCLISpecDocument(
        in directory: URL,
        name: String,
        machineLeafName: String?,
        about: String? = nil
    ) throws -> URL {
        try RalphMockCLITestSupport.writeJSONDocument(
            RalphMockCLITestSupport.cliSpecDocument(machineLeafName: machineLeafName, about: about),
            in: directory,
            name: name
        )
    }
}
