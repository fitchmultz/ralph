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
        let tempDir = try WorkspacePerformanceTestSupport.makeTempDir(prefix: "ralph-workspace-config-")
        defer { RalphCoreTestSupport.assertRemoved(tempDir) }

        let script = """
            #!/bin/sh
            if [ "$1" = "--no-color" ] && [ "$2" = "config" ] && [ "$3" = "show" ] && [ "$4" = "--format" ] && [ "$5" = "json" ]; then
              cat <<'JSON'
            {"agent":{"model":"kimi-code/kimi-for-coding","phases":2,"iterations":3}}
            JSON
              exit 0
            fi
            echo "unexpected args: $*" 1>&2
            exit 64
            """
        let scriptURL = try WorkspacePerformanceTestSupport.makeExecutableScript(in: tempDir, name: "mock-ralph", body: script)
        let client = try RalphCLIClient(executableURL: scriptURL)
        let workspace = Workspace(workingDirectoryURL: tempDir, client: client)

        await workspace.loadRunnerConfiguration(retryConfiguration: .minimal)

        XCTAssertEqual(workspace.runState.currentRunnerConfig?.model, "kimi-code/kimi-for-coding")
        XCTAssertEqual(workspace.runState.currentRunnerConfig?.phases, 2)
        XCTAssertEqual(workspace.runState.currentRunnerConfig?.maxIterations, 3)
    }

    func test_loadRunnerConfiguration_onFailure_clearsCurrentRunnerConfig() async throws {
        let tempDir = try WorkspacePerformanceTestSupport.makeTempDir(prefix: "ralph-workspace-config-failure-")
        defer { RalphCoreTestSupport.assertRemoved(tempDir) }

        let successScript = """
            #!/bin/sh
            if [ "$2" = "config" ] && [ "$3" = "show" ]; then
              echo '{"agent":{"model":"kimi-initial","phases":3,"iterations":2}}'
              exit 0
            fi
            exit 64
            """
        let successScriptURL = try WorkspacePerformanceTestSupport.makeExecutableScript(
            in: tempDir,
            name: "mock-ralph-success",
            body: successScript
        )
        let successClient = try RalphCLIClient(executableURL: successScriptURL)
        let workspace = Workspace(workingDirectoryURL: tempDir, client: successClient)
        await workspace.loadRunnerConfiguration(retryConfiguration: .minimal)
        XCTAssertEqual(workspace.runState.currentRunnerConfig?.model, "kimi-initial")
        XCTAssertEqual(workspace.runState.currentRunnerConfig?.phases, 3)
        XCTAssertEqual(workspace.runState.currentRunnerConfig?.maxIterations, 2)

        let failScript = """
            #!/bin/sh
            echo "config failed" 1>&2
            exit 1
            """
        let failScriptURL = try WorkspacePerformanceTestSupport.makeExecutableScript(
            in: tempDir,
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

    func test_setWorkingDirectory_refreshesRunnerConfiguration() async throws {
        let rootDir = try WorkspacePerformanceTestSupport.makeTempDir(prefix: "ralph-workspace-config-switch-")
        defer { RalphCoreTestSupport.assertRemoved(rootDir) }
        let workspaceADir = rootDir.appendingPathComponent("workspace-a", isDirectory: true)
        let workspaceBDir = rootDir.appendingPathComponent("workspace-b", isDirectory: true)
        try FileManager.default.createDirectory(at: workspaceADir, withIntermediateDirectories: true)
        try FileManager.default.createDirectory(at: workspaceBDir, withIntermediateDirectories: true)

        let switchScript = """
            #!/bin/sh
            if [ "$2" = "config" ] && [ "$3" = "show" ]; then
              case "$PWD" in
              */workspace-a)
                echo '{"agent":{"model":"model-a","phases":1,"iterations":1}}'
                ;;
              */workspace-b)
                echo '{"agent":{"model":"model-b","phases":2,"iterations":4}}'
                ;;
              *)
                echo '{"agent":{"model":"model-unknown","phases":3,"iterations":9}}'
                ;;
              esac
              exit 0
            fi
            exit 64
            """
        let scriptURL = try WorkspacePerformanceTestSupport.makeExecutableScript(
            in: rootDir,
            name: "mock-ralph-switch",
            body: switchScript
        )
        let client = try RalphCLIClient(executableURL: scriptURL)
        let workspace = Workspace(workingDirectoryURL: workspaceADir, client: client)

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

    func test_workspaceManager_adoptCLIExecutable_rejectsValidPathOverride() async throws {
        let manager = WorkspaceManager.shared
        let baselinePath = manager.client?.executableURL.standardizedFileURL.resolvingSymlinksInPath().path
        let tempDir = try WorkspacePerformanceTestSupport.makeTempDir(prefix: "ralph-workspace-manager-cli-")
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
