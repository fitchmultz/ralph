/**
 WorkspaceQueueRefreshTests

 Purpose:
 - Validate workspace queue refresh, analytics refresh, watcher refresh, and retargeting behavior.

 Responsibilities:
 - Validate workspace queue refresh, analytics refresh, watcher refresh, and retargeting behavior.

 Does not handle:
 - Low-level file-watcher retry behavior.
 - Direct task-creation path coverage.

 Usage:
 - Used by the RalphMac app or RalphCore tests through its owning feature surface.

 Invariants/assumptions callers must respect:
 - Tests initialize isolated temp workspaces and rely on deterministic queue/analytics convergence checks.
 */

import Foundation
import XCTest

@testable import RalphCore

@MainActor
final class WorkspaceQueueRefreshTests: RalphCoreTestCase {
    func test_workspaceInitialRefresh_populatesQueueWithoutEagerGraphOrAnalytics() async throws {
        var seedingWorkspace: Workspace!
        var workspace: Workspace!
        let workspaceURL = try WorkspaceTaskCreationTestSupport.makeTempDir(prefix: "ralph-workspace-refresh-initial-")
        defer { RalphCoreTestSupport.shutdownAndRemove(workspaceURL, seedingWorkspace, workspace) }

        let client = try RalphCLIClient(executableURL: try WorkspaceTaskCreationTestSupport.resolveRalphBinaryURL())
        try await WorkspaceTaskCreationTestSupport.runChecked(
            client: client,
            arguments: ["--no-color", "init", "--non-interactive"],
            currentDirectoryURL: workspaceURL
        )

        seedingWorkspace = Workspace(workingDirectoryURL: workspaceURL, client: client)
        try await seedingWorkspace.createTask(
            title: "Seed queue state",
            priority: .medium
        )
        try await seedingWorkspace.createTask(
            title: "Render analytics state",
            priority: .medium
        )

        workspace = Workspace(workingDirectoryURL: workspaceURL, client: client)

        let loaded = await RalphCoreTestSupport.waitUntil(timeout: .seconds(10)) {
            await MainActor.run {
                workspace.taskState.tasks.count == 2
                    && workspace.insightsState.graphData == nil
                    && workspace.insightsState.analytics.lastRefreshedAt == nil
            }
        }

        XCTAssertTrue(loaded)
        XCTAssertEqual(workspace.taskState.tasks.count, 2)
        XCTAssertNil(workspace.insightsState.graphData)
        XCTAssertNil(workspace.insightsState.analytics.lastRefreshedAt)
    }

    func test_workspaceWatcherExternalMutation_refreshesQueueGraphAndAnalytics() async throws {
        var workspace: Workspace!
        var writerWorkspace: Workspace!
        let workspaceURL = try WorkspaceTaskCreationTestSupport.makeTempDir(prefix: "ralph-workspace-refresh-watch-")
        defer { RalphCoreTestSupport.shutdownAndRemove(workspaceURL, workspace, writerWorkspace) }

        let client = try RalphCLIClient(executableURL: try WorkspaceTaskCreationTestSupport.resolveRalphBinaryURL())
        try await WorkspaceTaskCreationTestSupport.runChecked(
            client: client,
            arguments: ["--no-color", "init", "--non-interactive"],
            currentDirectoryURL: workspaceURL
        )

        workspace = Workspace(workingDirectoryURL: workspaceURL, client: client)
        writerWorkspace = Workspace(workingDirectoryURL: workspaceURL, client: client)

        let loadedEmptyState = await RalphCoreTestSupport.waitUntil(timeout: .seconds(10)) {
            await MainActor.run {
                workspace.taskState.tasks.isEmpty
                    && workspace.insightsState.graphData == nil
                    && workspace.insightsState.analytics.lastRefreshedAt == nil
                    && workspace.diagnosticsState.watcherHealth.isWatching
            }
        }

        XCTAssertTrue(loadedEmptyState)

        try await writerWorkspace.createTask(
            title: "Observe watcher refresh",
            priority: .medium
        )
        try await writerWorkspace.createTask(
            title: "Update analytics after mutation",
            priority: .medium
        )

        let refreshed = await RalphCoreTestSupport.waitUntil(timeout: .seconds(10)) {
            await MainActor.run {
                workspace.taskState.tasks.count == 2
                    && workspace.insightsState.graphData?.summary.totalTasks == 2
                    && workspace.insightsState.analytics.queueStatsValue?.summary.active == 2
                    && workspace.taskState.lastQueueRefreshEvent?.source == .externalFileChange
            }
        }

        XCTAssertTrue(refreshed)
        XCTAssertEqual(workspace.taskState.lastQueueRefreshEvent?.highlightedTaskIDs.count, 2)
    }

    func test_workspaceWatcherAtomicQueueReplacement_refreshesQueueGraphAndAnalytics() async throws {
        var workspace: Workspace!
        let workspaceURL = try WorkspaceTaskCreationTestSupport.makeTempDir(prefix: "ralph-workspace-refresh-replace-")
        defer { RalphCoreTestSupport.shutdownAndRemove(workspaceURL, workspace) }

        let client = try RalphCLIClient(executableURL: try WorkspaceTaskCreationTestSupport.resolveRalphBinaryURL())
        try await WorkspaceTaskCreationTestSupport.runChecked(
            client: client,
            arguments: ["--no-color", "init", "--non-interactive"],
            currentDirectoryURL: workspaceURL
        )

        workspace = Workspace(workingDirectoryURL: workspaceURL, client: client)

        let loadedEmptyState = await RalphCoreTestSupport.waitUntil(timeout: .seconds(10)) {
            await MainActor.run {
                workspace.taskState.tasks.isEmpty
                    && workspace.insightsState.graphData == nil
                    && workspace.insightsState.analytics.lastRefreshedAt == nil
                    && workspace.diagnosticsState.watcherHealth.isWatching
            }
        }

        XCTAssertTrue(loadedEmptyState)

        let replacementURL = workspaceURL.appendingPathComponent("queue-replacement.jsonc", isDirectory: false)
        try WorkspaceTaskCreationTestSupport.writeQueueDocument(
            to: replacementURL,
            tasks: [
                RalphTask(
                    id: "RQ-9001",
                    status: .todo,
                    title: "Atomic replace task",
                    priority: .high,
                    tags: ["watcher", "replace"],
                    createdAt: ISO8601DateFormatter().date(from: "2026-03-12T00:00:00Z"),
                    updatedAt: ISO8601DateFormatter().date(from: "2026-03-12T00:00:00Z")
                )
            ]
        )
        defer { XCTAssertNoThrow(try WorkspaceTaskCreationTestSupport.removeItemIfExists(replacementURL)) }

        _ = try FileManager.default.replaceItemAt(
            workspaceURL.appendingPathComponent(".ralph/queue.jsonc", isDirectory: false),
            withItemAt: replacementURL
        )

        let queueRefreshed = await RalphCoreTestSupport.waitUntil(timeout: .seconds(10)) {
            await MainActor.run {
                workspace.taskState.tasks.map(\.id) == ["RQ-9001"]
                    && workspace.taskState.lastQueueRefreshEvent?.source == .externalFileChange
            }
        }

        let queueTaskIDs = await MainActor.run { workspace.taskState.tasks.map { $0.id } }
        let refreshEvent = await MainActor.run { workspace.taskState.lastQueueRefreshEvent }
        XCTAssertTrue(
            queueRefreshed,
            "queue tasks=\(queueTaskIDs) event=\(String(describing: refreshEvent))"
        )

        let derivedViewsRefreshed = await RalphCoreTestSupport.waitUntil(timeout: .seconds(10)) {
            await MainActor.run {
                workspace.insightsState.graphData?.summary.totalTasks == 1
                    && workspace.insightsState.analytics.queueStatsValue?.summary.active == 1
            }
        }

        let graphTotalTasks = await MainActor.run { workspace.insightsState.graphData?.summary.totalTasks }
        let analyticsActiveCount = await MainActor.run { workspace.insightsState.analytics.queueStatsValue?.summary.active }
        XCTAssertTrue(
            derivedViewsRefreshed,
            "graph=\(String(describing: graphTotalTasks)) analytics=\(String(describing: analyticsActiveCount))"
        )
    }

    func test_workspaceRetarget_refreshesQueueWithoutEagerGraphOrAnalyticsForNewDirectory() async throws {
        var populatedWorkspace: Workspace!
        var workspace: Workspace!
        let emptyWorkspaceURL = try WorkspaceTaskCreationTestSupport.makeTempDir(prefix: "ralph-workspace-retarget-empty-")
        let populatedWorkspaceURL = try WorkspaceTaskCreationTestSupport.makeTempDir(prefix: "ralph-workspace-retarget-populated-")
        defer {
            RalphCoreTestSupport.shutdownAndRemove(emptyWorkspaceURL, workspace)
            RalphCoreTestSupport.shutdownAndRemove(populatedWorkspaceURL, populatedWorkspace)
        }

        let client = try RalphCLIClient(executableURL: try WorkspaceTaskCreationTestSupport.resolveRalphBinaryURL())
        try await WorkspaceTaskCreationTestSupport.runChecked(
            client: client,
            arguments: ["--no-color", "init", "--non-interactive"],
            currentDirectoryURL: emptyWorkspaceURL
        )
        try await WorkspaceTaskCreationTestSupport.runChecked(
            client: client,
            arguments: ["--no-color", "init", "--non-interactive"],
            currentDirectoryURL: populatedWorkspaceURL
        )

        populatedWorkspace = Workspace(workingDirectoryURL: populatedWorkspaceURL, client: client)
        try await populatedWorkspace.createTask(
            title: "Switch workspace truth",
            priority: .medium
        )

        workspace = Workspace(workingDirectoryURL: emptyWorkspaceURL, client: client)

        let loadedInitialState = await RalphCoreTestSupport.waitUntil(timeout: .seconds(10)) {
            await MainActor.run {
                workspace.taskState.tasks.isEmpty
                    && workspace.insightsState.graphData == nil
                    && workspace.insightsState.analytics.lastRefreshedAt == nil
            }
        }
        XCTAssertTrue(loadedInitialState)

        workspace.setWorkingDirectory(populatedWorkspaceURL)

        let retargeted = await RalphCoreTestSupport.waitUntil(timeout: .seconds(10)) {
            await MainActor.run {
                workspace.workingDirectoryURL == Workspace.normalizedWorkingDirectoryURL(populatedWorkspaceURL)
                    && workspace.taskState.tasks.count == 1
                    && workspace.insightsState.graphData == nil
                    && workspace.insightsState.analytics.lastRefreshedAt == nil
            }
        }

        XCTAssertTrue(retargeted)
    }

    func test_workspaceOverviewWatcherRetargetsWhenConfigMovesQueuePath() async throws {
        var workspace: Workspace!
        let rootURL = try WorkspaceTaskCreationTestSupport.makeTempDir(prefix: "ralph-workspace-overview-path-switch-")
        let workspaceURL = rootURL.appendingPathComponent("workspace", isDirectory: true)
        let customStateURL = workspaceURL.appendingPathComponent("custom-state", isDirectory: true)
        defer { RalphCoreTestSupport.shutdownAndRemove(rootURL, workspace) }

        try FileManager.default.createDirectory(at: customStateURL, withIntermediateDirectories: true)
        try FileManager.default.createDirectory(at: workspaceURL.appendingPathComponent(".ralph", isDirectory: true), withIntermediateDirectories: true)

        let initialTask = RalphMockCLITestSupport.task(id: "RQ-OVERVIEW-1", status: .todo, title: "Initial overview task", priority: .medium, createdAt: "2026-04-25T00:00:00Z", updatedAt: "2026-04-25T00:00:00Z")
        let switchedTask = RalphMockCLITestSupport.task(id: "RQ-OVERVIEW-2", status: .todo, title: "Switched queue task", priority: .high, createdAt: "2026-04-25T01:00:00Z", updatedAt: "2026-04-25T01:00:00Z")
        let defaultQueueURL = workspaceURL.appendingPathComponent(".ralph/queue.jsonc", isDirectory: false)
        let defaultDoneURL = workspaceURL.appendingPathComponent(".ralph/done.jsonc", isDirectory: false)
        let customQueueURL = customStateURL.appendingPathComponent("tasks.jsonc", isDirectory: false)
        let customDoneURL = customStateURL.appendingPathComponent("archive.jsonc", isDirectory: false)
        let projectConfigURL = workspaceURL.appendingPathComponent(".ralph/config.jsonc", isDirectory: false)
        try WorkspaceTaskCreationTestSupport.writeQueueDocument(to: defaultQueueURL, tasks: [initialTask])
        try WorkspaceTaskCreationTestSupport.writeQueueDocument(to: customQueueURL, tasks: [switchedTask])
        try "[]\n".write(to: defaultDoneURL, atomically: true, encoding: .utf8)
        try "[]\n".write(to: customDoneURL, atomically: true, encoding: .utf8)
        try "{}\n".write(to: projectConfigURL, atomically: true, encoding: .utf8)

        let switchedPaths = RalphMockCLITestSupport.MockResolvedPathOverrides(queueURL: customQueueURL, doneURL: customDoneURL, projectConfigURL: projectConfigURL)
        let overviewURL = try WorkspaceRunnerConfigurationTestSupport.writeWorkspaceOverviewDocument(in: rootURL, name: "overview.json", workspaceURL: workspaceURL, activeTasks: [initialTask], nextRunnableTaskID: initialTask.id, model: "overview-model")
        let configResolveCurrentURL = rootURL.appendingPathComponent("config-resolve-current.json", isDirectory: false)
        try FileManager.default.copyItem(at: try WorkspaceRunnerConfigurationTestSupport.writeConfigResolveDocument(in: rootURL, name: "config-resolve-switched.json", workspaceURL: workspaceURL, model: "overview-model", pathOverrides: switchedPaths), to: configResolveCurrentURL)
        let queueReadCurrentURL = rootURL.appendingPathComponent("queue-read-current.json", isDirectory: false)
        try FileManager.default.copyItem(at: try WorkspaceRunnerConfigurationTestSupport.writeQueueReadDocument(in: rootURL, name: "queue-read-switched.json", workspaceURL: workspaceURL, activeTasks: [switchedTask], nextRunnableTaskID: switchedTask.id, pathOverrides: switchedPaths), to: queueReadCurrentURL)
        let graphCurrentURL = rootURL.appendingPathComponent("graph-current.json", isDirectory: false)
        try FileManager.default.copyItem(at: try WorkspaceRunnerConfigurationTestSupport.writeGraphDocument(in: rootURL, name: "graph-switched.json", tasks: [RalphMockCLITestSupport.graphNode(id: switchedTask.id, title: switchedTask.title)]), to: graphCurrentURL)
        let dashboardCurrentURL = rootURL.appendingPathComponent("dashboard-current.json", isDirectory: false)
        try """
        {"version":1,"dashboard":{"window_days":7,"generated_at":"2026-04-25T01:00:00Z","sections":{"productivity_summary":{"status":"unavailable","data":null,"error_message":"not needed"},"productivity_velocity":{"status":"unavailable","data":null,"error_message":"not needed"},"burndown":{"status":"unavailable","data":null,"error_message":"not needed"},"queue_stats":{"status":"ok","data":{"summary":{"total":1,"done":0,"rejected":0,"terminal":0,"active":1,"terminal_rate":0},"tag_breakdown":[]},"error_message":null},"history":{"status":"unavailable","data":null,"error_message":"not needed"}}}}
        """.write(to: dashboardCurrentURL, atomically: true, encoding: .utf8)

        let scriptURL = try RalphMockCLITestSupport.makeExecutableScript(in: rootURL, name: "mock-ralph-overview-path-switch", body: """
            #!/bin/sh
            set -eu
            if [ "$1" = "--no-color" ]; then shift; fi
            if [ "$1" = "machine" ] && [ "$2" = "workspace" ] && [ "$3" = "overview" ]; then cat "\(overviewURL.path)"; exit 0; fi
            if [ "$1" = "machine" ] && [ "$2" = "config" ] && [ "$3" = "resolve" ]; then cat "\(configResolveCurrentURL.path)"; exit 0; fi
            if [ "$1" = "machine" ] && [ "$2" = "queue" ] && [ "$3" = "read" ]; then cat "\(queueReadCurrentURL.path)"; exit 0; fi
            if [ "$1" = "machine" ] && [ "$2" = "queue" ] && [ "$3" = "graph" ]; then cat "\(graphCurrentURL.path)"; exit 0; fi
            if [ "$1" = "machine" ] && [ "$2" = "queue" ] && [ "$3" = "dashboard" ]; then cat "\(dashboardCurrentURL.path)"; exit 0; fi
            echo "unexpected args: $*" >&2
            exit 64
            """)
        workspace = Workspace(workingDirectoryURL: workspaceURL, client: try RalphCLIClient(executableURL: scriptURL), bootstrapRepositoryStateOnInit: false)

        await workspace.refreshWorkspaceOverviewState(retryConfiguration: .minimal)
        let watcherStarted = await RalphCoreTestSupport.waitUntil(timeout: .seconds(10)) {
            await MainActor.run { workspace.diagnosticsState.watcherHealth.isWatching }
        }
        XCTAssertEqual(workspace.taskState.tasks.map(\.id), [initialTask.id])
        XCTAssertEqual(workspace.queueFileURL, defaultQueueURL)
        XCTAssertTrue(watcherStarted)

        try "{ \"version\": 2, \"queue\": { \"file\": \"custom-state/tasks.jsonc\", \"done_file\": \"custom-state/archive.jsonc\" } }\n".write(to: projectConfigURL, atomically: true, encoding: .utf8)

        let switched = await RalphCoreTestSupport.waitUntil(timeout: .seconds(10)) {
            await MainActor.run {
                workspace.queueFileURL == customQueueURL
                    && workspace.doneFileURL == customDoneURL
                    && workspace.taskState.tasks.map(\.id) == [switchedTask.id]
                    && workspace.taskState.lastQueueRefreshEvent?.source == .externalFileChange
            }
        }

        XCTAssertTrue(switched)
    }

    func test_loadTasks_resolvesCustomQueuePathWhenWorkspaceOverviewCapabilityIsUnsupported() async throws {
        var workspace: Workspace!
        let rootURL = try WorkspaceTaskCreationTestSupport.makeTempDir(prefix: "ralph-workspace-custom-queue-load-")
        let workspaceURL = rootURL.appendingPathComponent("workspace", isDirectory: true)
        let customStateURL = workspaceURL.appendingPathComponent("custom-state", isDirectory: true)
        defer { RalphCoreTestSupport.shutdownAndRemove(rootURL, workspace) }

        try FileManager.default.createDirectory(at: customStateURL, withIntermediateDirectories: true)
        try FileManager.default.createDirectory(
            at: workspaceURL.appendingPathComponent(".ralph", isDirectory: true),
            withIntermediateDirectories: true
        )

        let customQueueURL = customStateURL.appendingPathComponent("queue.jsonc", isDirectory: false)
        let customDoneURL = customStateURL.appendingPathComponent("done.jsonc", isDirectory: false)
        let projectConfigURL = workspaceURL.appendingPathComponent(".ralph/config.jsonc", isDirectory: false)
        try "[]\n".write(to: customDoneURL, atomically: true, encoding: .utf8)
        try "{ \"version\": 2, \"queue\": { \"file\": \"custom-state/queue.jsonc\", \"done_file\": \"custom-state/done.jsonc\" } }\n"
            .write(to: projectConfigURL, atomically: true, encoding: .utf8)

        let initialTask = RalphMockCLITestSupport.task(
            id: "RQ-CUSTOM-1",
            status: .todo,
            title: "Initial custom queue task",
            priority: .medium,
            createdAt: "2026-04-25T00:00:00Z",
            updatedAt: "2026-04-25T00:00:00Z"
        )
        let updatedTask = RalphMockCLITestSupport.task(
            id: "RQ-CUSTOM-2",
            status: .todo,
            title: "Updated custom queue task",
            priority: .high,
            createdAt: "2026-04-25T01:00:00Z",
            updatedAt: "2026-04-25T01:00:00Z"
        )
        try WorkspaceTaskCreationTestSupport.writeQueueDocument(to: customQueueURL, tasks: [initialTask])

        let pathOverrides = RalphMockCLITestSupport.MockResolvedPathOverrides(
            queueURL: customQueueURL,
            doneURL: customDoneURL,
            projectConfigURL: projectConfigURL
        )
        let configResolveURL = try WorkspaceRunnerConfigurationTestSupport.writeConfigResolveDocument(
            in: rootURL,
            name: "config-resolve.json",
            workspaceURL: workspaceURL,
            model: "custom-path-model",
            pathOverrides: pathOverrides
        )
        let queueReadURL = try WorkspaceRunnerConfigurationTestSupport.writeQueueReadDocument(
            in: rootURL,
            name: "queue-read.json",
            workspaceURL: workspaceURL,
            activeTasks: [initialTask],
            nextRunnableTaskID: initialTask.id,
            pathOverrides: pathOverrides
        )
        let queueReadUpdatedURL = try WorkspaceRunnerConfigurationTestSupport.writeQueueReadDocument(
            in: rootURL,
            name: "queue-read-updated.json",
            workspaceURL: workspaceURL,
            activeTasks: [updatedTask],
            nextRunnableTaskID: updatedTask.id,
            pathOverrides: pathOverrides
        )
        let graphReadURL = try WorkspaceRunnerConfigurationTestSupport.writeGraphDocument(
            in: rootURL,
            name: "graph-read.json",
            tasks: [RalphMockCLITestSupport.graphNode(id: updatedTask.id, title: updatedTask.title)]
        )
        let dashboardReadURL = rootURL.appendingPathComponent("dashboard-read.json", isDirectory: false)
        try """
        {
          "version": 1,
          "dashboard": {
            "window_days": 7,
            "generated_at": "2026-04-25T01:00:00Z",
            "sections": {
              "productivity_summary": { "status": "unavailable", "data": null, "error_message": "not needed" },
              "productivity_velocity": { "status": "unavailable", "data": null, "error_message": "not needed" },
              "burndown": { "status": "unavailable", "data": null, "error_message": "not needed" },
              "queue_stats": {
                "status": "ok",
                "data": {
                  "summary": {
                    "total": 1,
                    "done": 0,
                    "rejected": 0,
                    "terminal": 0,
                    "active": 1,
                    "terminal_rate": 0
                  },
                  "tag_breakdown": []
                },
                "error_message": null
              },
              "history": { "status": "unavailable", "data": null, "error_message": "not needed" }
            }
          }
        }
        """.write(to: dashboardReadURL, atomically: true, encoding: .utf8)

        let queueReadCurrentURL = rootURL.appendingPathComponent("queue-read-current.json", isDirectory: false)
        try FileManager.default.copyItem(at: queueReadURL, to: queueReadCurrentURL)
        let cliSpecURL = try RalphMockCLITestSupport.writeJSONDocument(
            Self.workspaceOverviewCapabilitySpecDocument(supportsWorkspaceOverview: false),
            in: rootURL,
            name: "cli-spec-no-workspace-overview.json"
        )

        let script = """
            #!/bin/sh
            set -eu
            if [ "$1" = "--no-color" ]; then
              shift
            fi
            if [ "$1" = "machine" ] && [ "$2" = "workspace" ] && [ "$3" = "overview" ]; then
              echo "unrecognized subcommand 'overview'" >&2
              exit 64
            fi
            if [ "$1" = "machine" ] && [ "$2" = "cli-spec" ]; then
              cat "\(cliSpecURL.path)"
              exit 0
            fi
            if [ "$1" = "machine" ] && [ "$2" = "config" ] && [ "$3" = "resolve" ]; then
              cat "\(configResolveURL.path)"
              exit 0
            fi
            if [ "$1" = "machine" ] && [ "$2" = "queue" ] && [ "$3" = "read" ]; then
              cat "\(queueReadCurrentURL.path)"
              exit 0
            fi
            if [ "$1" = "machine" ] && [ "$2" = "queue" ] && [ "$3" = "graph" ]; then
              cat "\(graphReadURL.path)"
              exit 0
            fi
            if [ "$1" = "machine" ] && [ "$2" = "queue" ] && [ "$3" = "dashboard" ]; then
              cat "\(dashboardReadURL.path)"
              exit 0
            fi
            echo "unexpected args: $*" >&2
            exit 64
            """
        let scriptURL = try RalphMockCLITestSupport.makeExecutableScript(
            in: rootURL,
            name: "mock-ralph-custom-queue-load",
            body: script
        )
        workspace = Workspace(
            workingDirectoryURL: workspaceURL,
            client: try RalphCLIClient(executableURL: scriptURL),
            bootstrapRepositoryStateOnInit: false
        )

        await workspace.refreshWorkspaceOverviewState(retryConfiguration: .minimal)

        XCTAssertEqual(workspace.taskState.tasks.map(\.id), [initialTask.id])
        XCTAssertEqual(workspace.queueFileURL, customQueueURL)
        XCTAssertFalse(
            FileManager.default.fileExists(
                atPath: workspaceURL.appendingPathComponent(".ralph/queue.jsonc", isDirectory: false).path
            )
        )
        XCTAssertTrue(workspace.diagnosticsState.watcherHealth.isWatching)

        try WorkspaceTaskCreationTestSupport.removeItemIfExists(queueReadCurrentURL)
        try FileManager.default.copyItem(at: queueReadUpdatedURL, to: queueReadCurrentURL)
        try WorkspaceTaskCreationTestSupport.writeQueueDocument(to: customQueueURL, tasks: [updatedTask])

        let refreshed = await RalphCoreTestSupport.waitUntil(timeout: .seconds(10)) {
            await MainActor.run {
                workspace.taskState.tasks.map(\.id) == [updatedTask.id]
                    && workspace.taskState.lastQueueRefreshEvent?.source == .externalFileChange
            }
        }

        XCTAssertTrue(refreshed)
    }

    func test_refreshWorkspaceOverview_doesNotFallbackWhenCliSpecSupportsWorkspaceOverview() async throws {
        var workspace: Workspace!
        let rootURL = try WorkspaceTaskCreationTestSupport.makeTempDir(prefix: "ralph-workspace-overview-capability-supported-")
        let workspaceURL = rootURL.appendingPathComponent("workspace", isDirectory: true)
        defer { RalphCoreTestSupport.shutdownAndRemove(rootURL, workspace) }

        try FileManager.default.createDirectory(
            at: workspaceURL.appendingPathComponent(".ralph", isDirectory: true),
            withIntermediateDirectories: true
        )

        let fallbackTask = RalphMockCLITestSupport.task(
            id: "RQ-OVERVIEW-FALLBACK",
            status: .todo,
            title: "Should not load",
            priority: .medium,
            createdAt: "2026-04-26T00:00:00Z",
            updatedAt: "2026-04-26T00:00:00Z"
        )
        let queueReadURL = try WorkspaceRunnerConfigurationTestSupport.writeQueueReadDocument(
            in: rootURL,
            name: "queue-read.json",
            workspaceURL: workspaceURL,
            activeTasks: [fallbackTask],
            nextRunnableTaskID: fallbackTask.id
        )
        let configResolveURL = try WorkspaceRunnerConfigurationTestSupport.writeConfigResolveDocument(
            in: rootURL,
            name: "config-resolve.json",
            workspaceURL: workspaceURL,
            model: "capability-supported-model"
        )
        let cliSpecURL = try RalphMockCLITestSupport.writeJSONDocument(
            Self.workspaceOverviewCapabilitySpecDocument(supportsWorkspaceOverview: true),
            in: rootURL,
            name: "cli-spec-with-workspace-overview.json"
        )
        let commandLogURL = rootURL.appendingPathComponent("command-log.txt", isDirectory: false)

        let script = """
            #!/bin/sh
            set -eu
            if [ "$1" = "--no-color" ]; then
              shift
            fi
            printf '%s\n' "$*" >> "\(commandLogURL.path)"
            if [ "$1" = "machine" ] && [ "$2" = "workspace" ] && [ "$3" = "overview" ]; then
              echo "usage: ralph machine workspace overview [OPTIONS]" >&2
              exit 64
            fi
            if [ "$1" = "machine" ] && [ "$2" = "cli-spec" ]; then
              cat "\(cliSpecURL.path)"
              exit 0
            fi
            if [ "$1" = "machine" ] && [ "$2" = "queue" ] && [ "$3" = "read" ]; then
              cat "\(queueReadURL.path)"
              exit 0
            fi
            if [ "$1" = "machine" ] && [ "$2" = "config" ] && [ "$3" = "resolve" ]; then
              cat "\(configResolveURL.path)"
              exit 0
            fi
            echo "unexpected args: $*" >&2
            exit 64
            """

        let scriptURL = try RalphMockCLITestSupport.makeExecutableScript(
            in: rootURL,
            name: "mock-ralph-overview-capability-supported",
            body: script
        )
        workspace = Workspace(
            workingDirectoryURL: workspaceURL,
            client: try RalphCLIClient(executableURL: scriptURL),
            bootstrapRepositoryStateOnInit: false
        )

        await workspace.refreshWorkspaceOverviewState(retryConfiguration: .minimal)

        XCTAssertTrue(workspace.taskState.tasks.isEmpty)
        XCTAssertFalse(workspace.diagnosticsState.watcherHealth.isWatching)
        XCTAssertTrue(
            workspace.taskState.tasksErrorMessage?.contains("usage: ralph machine workspace overview [OPTIONS]") ?? false
        )
        XCTAssertTrue(
            workspace.runState.runnerConfigErrorMessage?.contains("usage: ralph machine workspace overview [OPTIONS]") ?? false
        )

        let commandLog = try String(contentsOf: commandLogURL, encoding: .utf8)
        XCTAssertTrue(commandLog.contains("machine workspace overview"))
        XCTAssertTrue(commandLog.contains("machine cli-spec"))
        XCTAssertFalse(commandLog.contains("machine queue read"))
        XCTAssertFalse(commandLog.contains("machine config resolve"))
    }

    func test_refreshWorkspaceOverview_structuredMachineErrorDoesNotTriggerFallback() async throws {
        var workspace: Workspace!
        let rootURL = try WorkspaceTaskCreationTestSupport.makeTempDir(prefix: "ralph-workspace-overview-machine-error-")
        let workspaceURL = rootURL.appendingPathComponent("workspace", isDirectory: true)
        defer { RalphCoreTestSupport.shutdownAndRemove(rootURL, workspace) }

        try FileManager.default.createDirectory(
            at: workspaceURL.appendingPathComponent(".ralph", isDirectory: true),
            withIntermediateDirectories: true
        )

        let fallbackTask = RalphMockCLITestSupport.task(
            id: "RQ-OVERVIEW-MACHINE-ERROR",
            status: .todo,
            title: "Should not load",
            priority: .medium,
            createdAt: "2026-04-26T00:00:00Z",
            updatedAt: "2026-04-26T00:00:00Z"
        )
        let queueReadURL = try WorkspaceRunnerConfigurationTestSupport.writeQueueReadDocument(
            in: rootURL,
            name: "queue-read.json",
            workspaceURL: workspaceURL,
            activeTasks: [fallbackTask],
            nextRunnableTaskID: fallbackTask.id
        )
        let configResolveURL = try WorkspaceRunnerConfigurationTestSupport.writeConfigResolveDocument(
            in: rootURL,
            name: "config-resolve.json",
            workspaceURL: workspaceURL,
            model: "machine-error-model"
        )
        let cliSpecURL = try RalphMockCLITestSupport.writeJSONDocument(
            Self.workspaceOverviewCapabilitySpecDocument(supportsWorkspaceOverview: false),
            in: rootURL,
            name: "cli-spec-no-workspace-overview.json"
        )
        let machineError = MachineErrorDocument(
            version: MachineErrorDocument.expectedVersion,
            code: .resourceBusy,
            message: "Workspace overview failed.",
            detail: "mocked machine contract failure",
            retryable: false
        )
        let machineErrorURL = try RalphMockCLITestSupport.writeJSONDocument(
            machineError,
            in: rootURL,
            name: "workspace-overview-machine-error.json"
        )
        let commandLogURL = rootURL.appendingPathComponent("command-log.txt", isDirectory: false)

        let script = """
            #!/bin/sh
            set -eu
            if [ "$1" = "--no-color" ]; then
              shift
            fi
            printf '%s\n' "$*" >> "\(commandLogURL.path)"
            if [ "$1" = "machine" ] && [ "$2" = "workspace" ] && [ "$3" = "overview" ]; then
              cat "\(machineErrorURL.path)" >&2
              exit 70
            fi
            if [ "$1" = "machine" ] && [ "$2" = "cli-spec" ]; then
              cat "\(cliSpecURL.path)"
              exit 0
            fi
            if [ "$1" = "machine" ] && [ "$2" = "queue" ] && [ "$3" = "read" ]; then
              cat "\(queueReadURL.path)"
              exit 0
            fi
            if [ "$1" = "machine" ] && [ "$2" = "config" ] && [ "$3" = "resolve" ]; then
              cat "\(configResolveURL.path)"
              exit 0
            fi
            echo "unexpected args: $*" >&2
            exit 64
            """

        let scriptURL = try RalphMockCLITestSupport.makeExecutableScript(
            in: rootURL,
            name: "mock-ralph-overview-machine-error",
            body: script
        )
        workspace = Workspace(
            workingDirectoryURL: workspaceURL,
            client: try RalphCLIClient(executableURL: scriptURL),
            bootstrapRepositoryStateOnInit: false
        )

        await workspace.refreshWorkspaceOverviewState(retryConfiguration: .minimal)

        XCTAssertTrue(workspace.taskState.tasks.isEmpty)
        XCTAssertEqual(workspace.taskState.tasksErrorMessage, machineError.message)
        XCTAssertEqual(workspace.runState.runnerConfigErrorMessage, machineError.message)

        let commandLog = try String(contentsOf: commandLogURL, encoding: .utf8)
        XCTAssertTrue(commandLog.contains("machine workspace overview"))
        XCTAssertFalse(commandLog.contains("machine cli-spec"))
        XCTAssertFalse(commandLog.contains("machine queue read"))
        XCTAssertFalse(commandLog.contains("machine config resolve"))
    }
}

private extension WorkspaceQueueRefreshTests {
    static func workspaceOverviewCapabilitySpecDocument(
        supportsWorkspaceOverview: Bool
    ) -> MachineCLISpecDocument {
        let queueCommand = commandSpec(
            name: "queue",
            path: ["ralph", "machine", "queue"]
        )
        let workspaceOverviewCommand = commandSpec(
            name: "overview",
            path: ["ralph", "machine", "workspace", "overview"]
        )
        let workspaceCommand = commandSpec(
            name: "workspace",
            path: ["ralph", "machine", "workspace"],
            subcommands: supportsWorkspaceOverview ? [workspaceOverviewCommand] : []
        )
        let machineSubcommands = supportsWorkspaceOverview
            ? [queueCommand, workspaceCommand]
            : [queueCommand]

        return MachineCLISpecDocument(
            version: RalphMachineContract.cliSpecVersion,
            spec: RalphCLISpecDocument(
                version: RalphCLISpecDocument.expectedVersion,
                root: commandSpec(
                    name: "ralph",
                    path: ["ralph"],
                    subcommands: [
                        commandSpec(
                            name: "machine",
                            path: ["ralph", "machine"],
                            subcommands: machineSubcommands
                        )
                    ]
                )
            )
        )
    }

    static func commandSpec(
        name: String,
        path: [String],
        subcommands: [RalphCLICommandSpec] = []
    ) -> RalphCLICommandSpec {
        RalphCLICommandSpec(
            name: name,
            path: path,
            about: nil,
            longAbout: nil,
            afterLongHelp: nil,
            hidden: false,
            args: [],
            subcommands: subcommands
        )
    }
}
