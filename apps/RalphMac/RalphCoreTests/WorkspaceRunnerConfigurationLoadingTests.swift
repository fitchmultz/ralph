/**
 WorkspaceRunnerConfigurationLoadingTests

 Responsibilities:
 - Validate runner-configuration loading, decoding, failure clearing, and shutdown suppression behavior.

 Does not handle:
 - Working-directory retargeting coverage.
 - Workspace-manager CLI override behavior.

 Invariants/assumptions callers must respect:
 - Mock CLIs emulate only the config-resolve surfaces asserted by each test.
 */

import XCTest

@testable import RalphCore

@MainActor
final class WorkspaceRunnerConfigurationLoadingTests: WorkspacePerformanceTestCase {
    func test_loadRunnerConfiguration_setsCurrentRunnerConfig() async throws {
        var workspace: Workspace!
        let fixture = try RalphMockCLITestSupport.makeFixture(prefix: "ralph-workspace-config")
        defer { RalphCoreTestSupport.shutdownAndRemove(fixture.rootURL, workspace) }

        let configResolveURL = try WorkspaceRunnerConfigurationTestSupport.writeConfigResolveDocument(
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
        workspace = RalphMockCLITestSupport.makeWorkspaceWithoutInitialRefresh(
            workingDirectoryURL: fixture.workspaceURL,
            client: client
        )

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
        let configResolveURL = try WorkspaceRunnerConfigurationTestSupport.writeConfigResolveDocument(
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
        workspace = RalphMockCLITestSupport.makeWorkspaceWithoutInitialRefresh(
            workingDirectoryURL: fixture.workspaceURL,
            client: client
        )

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

        let successConfigURL = try WorkspaceRunnerConfigurationTestSupport.writeConfigResolveDocument(
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
        workspace = RalphMockCLITestSupport.makeWorkspaceWithoutInitialRefresh(
            workingDirectoryURL: fixture.workspaceURL,
            client: successClient
        )

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
        let configResolveURL = try WorkspaceRunnerConfigurationTestSupport.writeConfigResolveDocument(
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
}
