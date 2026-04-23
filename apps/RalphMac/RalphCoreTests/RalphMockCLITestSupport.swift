/**
 RalphMockCLITestSupport

 Responsibilities:
 - Centralize mock Ralph CLI script creation for RalphCore tests.
 - Generate machine-readable config, queue, graph, and CLI-spec fixtures with workspace-resolved paths.
 - Build test workspaces that opt into mock clients without triggering unrelated repository refreshes.
 - Keep test payloads aligned with RalphCore Codable contracts so path and JSON shape drift is caught in one place.

 Does not handle:
 - Production CLI behavior.
 - UI-test harness setup.
 - Highly stateful shell routing beyond writing the mock executable and fixture documents.

 Invariants/assumptions callers must respect:
 - Encoded task dates use ISO8601 to match production decode paths.
 - Machine payloads always derive queue/config/done paths under `.ralph` from the provided workspace URL.
 - Tests may compose custom shell routing, but fixture documents should come from this helper instead of ad hoc inline JSON.
 */

import Foundation
@testable import RalphCore

enum RalphMockCLITestSupport {
    struct Fixture {
        let rootURL: URL
        let workspaceURL: URL
        let ralphDirectoryURL: URL
        let queueURL: URL
        let doneURL: URL
        let configURL: URL
        let scriptURL: URL
        let logURL: URL?
    }

    static let defaultSafetySummary = MachineConfigSafetySummary(
        repoTrusted: true,
        dirtyRepo: false,
        gitPublishMode: "off",
        approvalMode: "default",
        ciGateEnabled: true,
        gitRevertMode: "ask",
        parallelConfigured: false,
        executionInteractivity: "noninteractive_streaming",
        interactiveApprovalSupported: false
    )

    static let emptyRunnability: RalphJSONValue = .object([:])

    static func makeFixture(
        prefix: String,
        workspaceName: String? = nil,
        scriptName: String = "mock-ralph",
        logFileName: String? = nil,
        seedQueueTasks: [RalphTask]? = nil,
        seedDoneTasks: [RalphTask]? = nil,
        createConfigFile: Bool = false
    ) throws -> Fixture {
        let rootURL = try RalphCoreTestSupport.makeTemporaryDirectory(prefix: prefix)
        let workspaceURL: URL
        if let workspaceName {
            workspaceURL = rootURL.appendingPathComponent(workspaceName, isDirectory: true)
            try FileManager.default.createDirectory(at: workspaceURL, withIntermediateDirectories: true)
        } else {
            workspaceURL = rootURL
        }

        let ralphDirectoryURL = workspaceURL.appendingPathComponent(".ralph", isDirectory: true)
        try FileManager.default.createDirectory(at: ralphDirectoryURL, withIntermediateDirectories: true)

        let queueURL = ralphDirectoryURL.appendingPathComponent("queue.jsonc", isDirectory: false)
        let doneURL = ralphDirectoryURL.appendingPathComponent("done.jsonc", isDirectory: false)
        let configURL = ralphDirectoryURL.appendingPathComponent("config.jsonc", isDirectory: false)
        if let seedQueueTasks {
            try writeQueueFile(in: workspaceURL, tasks: seedQueueTasks)
        }
        if let seedDoneTasks {
            try writeDoneFile(in: workspaceURL, tasks: seedDoneTasks)
        }
        if createConfigFile {
            try "{}\n".write(to: configURL, atomically: true, encoding: .utf8)
        }

        let scriptURL = rootURL.appendingPathComponent(scriptName, isDirectory: false)
        let logURL = logFileName.map { rootURL.appendingPathComponent($0, isDirectory: false) }

        return Fixture(
            rootURL: rootURL,
            workspaceURL: workspaceURL,
            ralphDirectoryURL: ralphDirectoryURL,
            queueURL: queueURL,
            doneURL: doneURL,
            configURL: configURL,
            scriptURL: scriptURL,
            logURL: logURL
        )
    }

    static func makeExecutableScript(in directory: URL, name: String = "mock-ralph", body: String) throws -> URL {
        let scriptURL = directory.appendingPathComponent(name, isDirectory: false)
        try body.write(to: scriptURL, atomically: true, encoding: .utf8)
        try FileManager.default.setAttributes(
            [.posixPermissions: NSNumber(value: Int16(0o755))],
            ofItemAtPath: scriptURL.path
        )
        return scriptURL
    }

    static func makeVersionAwareMockCLI(in directory: URL, name: String = "mock-ralph") throws -> URL {
        let script = """
            #!/bin/sh
            if [ "$1" = "--version" ] || [ "$1" = "version" ]; then
              echo "ralph \(VersionCompatibility.minimumCLIVersion)"
              exit 0
            fi
            if [ "$1" = "--no-color" ] && [ "$2" = "machine" ] && [ "$3" = "system" ] && [ "$4" = "info" ]; then
              echo '{"version":1,"cli_version":"\(VersionCompatibility.minimumCLIVersion)"}'
              exit 0
            fi
            echo "unexpected args: $*" 1>&2
            exit 64
            """
        return try makeExecutableScript(in: directory, name: name, body: script)
    }

    @MainActor
    static func makeWorkspaceWithoutInitialRefresh(
        workingDirectoryURL: URL,
        client: RalphCLIClient
    ) -> Workspace {
        let workspace = Workspace(workingDirectoryURL: workingDirectoryURL)
        workspace.client = client
        return workspace
    }

    static func resolvedPaths(for workspaceURL: URL) -> MachineQueuePaths {
        let workspacePath = workspaceURL.path
        return MachineQueuePaths(
            repoRoot: workspacePath,
            queuePath: workspaceURL.appendingPathComponent(".ralph/queue.jsonc", isDirectory: false).path,
            donePath: workspaceURL.appendingPathComponent(".ralph/done.jsonc", isDirectory: false).path,
            projectConfigPath: workspaceURL.appendingPathComponent(".ralph/config.jsonc", isDirectory: false).path,
            globalConfigPath: nil
        )
    }

    static func configResolveDocument(
        workspaceURL: URL,
        safety: MachineConfigSafetySummary = defaultSafetySummary,
        agent: AgentConfig = AgentConfig(),
        resumePreview: MachineResumeDecision? = nil
    ) -> MachineConfigResolveDocument {
        MachineConfigResolveDocument(
            version: 3,
            paths: resolvedPaths(for: workspaceURL),
            safety: safety,
            config: RalphConfig(agent: agent),
            resumePreview: resumePreview
        )
    }

    static func workspaceOverviewDocument(
        workspaceURL: URL,
        activeTasks: [RalphTask],
        doneTasks: [RalphTask] = [],
        nextRunnableTaskID: String? = nil,
        runnability: RalphJSONValue = emptyRunnability,
        safety: MachineConfigSafetySummary = defaultSafetySummary,
        agent: AgentConfig = AgentConfig(),
        resumePreview: MachineResumeDecision? = nil
    ) -> MachineWorkspaceOverviewDocument {
        MachineWorkspaceOverviewDocument(
            version: 1,
            queue: queueReadDocument(
                workspaceURL: workspaceURL,
                activeTasks: activeTasks,
                doneTasks: doneTasks,
                nextRunnableTaskID: nextRunnableTaskID,
                runnability: runnability
            ),
            config: configResolveDocument(
                workspaceURL: workspaceURL,
                safety: safety,
                agent: agent,
                resumePreview: resumePreview
            )
        )
    }

    static func queueReadDocument(
        workspaceURL: URL,
        activeTasks: [RalphTask],
        doneTasks: [RalphTask] = [],
        nextRunnableTaskID: String? = nil,
        runnability: RalphJSONValue = emptyRunnability
    ) -> MachineQueueReadDocument {
        MachineQueueReadDocument(
            version: 1,
            paths: resolvedPaths(for: workspaceURL),
            active: RalphTaskQueueDocument(tasks: activeTasks),
            done: RalphTaskQueueDocument(tasks: doneTasks),
            nextRunnableTaskID: nextRunnableTaskID,
            runnability: runnability
        )
    }

    static func graphReadDocument(
        tasks: [RalphGraphNode],
        runnableTasks: Int? = nil,
        blockedTasks: Int = 0,
        criticalPaths: [RalphCriticalPath] = []
    ) -> MachineGraphReadDocument {
        MachineGraphReadDocument(
            version: 1,
            graph: RalphGraphDocument(
                summary: RalphGraphSummary(
                    totalTasks: tasks.count,
                    runnableTasks: runnableTasks ?? tasks.count,
                    blockedTasks: blockedTasks
                ),
                criticalPaths: criticalPaths,
                tasks: tasks
            )
        )
    }

    static func cliSpecDocument(machineLeafName: String? = nil, about: String? = nil) -> MachineCLISpecDocument {
        let leafCommands: [RalphCLICommandSpec]
        if let machineLeafName {
            leafCommands = [
                RalphCLICommandSpec(
                    name: machineLeafName,
                    path: ["ralph", "machine", machineLeafName],
                    about: about,
                    longAbout: nil,
                    afterLongHelp: nil,
                    hidden: false,
                    args: [],
                    subcommands: []
                )
            ]
        } else {
            leafCommands = []
        }

        return MachineCLISpecDocument(
            version: 2,
            spec: RalphCLISpecDocument(
                version: 2,
                root: RalphCLICommandSpec(
                    name: "ralph",
                    path: ["ralph"],
                    about: nil,
                    longAbout: nil,
                    afterLongHelp: nil,
                    hidden: false,
                    args: [],
                    subcommands: [
                        RalphCLICommandSpec(
                            name: "machine",
                            path: ["ralph", "machine"],
                            about: "Machine",
                            longAbout: nil,
                            afterLongHelp: nil,
                            hidden: false,
                            args: [],
                            subcommands: leafCommands
                        )
                    ]
                )
            )
        )
    }

    static func graphNode(
        id: String,
        title: String,
        status: RalphTaskStatus = .todo,
        dependencies: [String] = [],
        dependents: [String] = [],
        critical: Bool = false
    ) -> RalphGraphNode {
        RalphGraphNode(
            id: id,
            title: title,
            status: status.rawValue,
            dependencies: dependencies,
            dependents: dependents,
            isCritical: critical
        )
    }

    static func task(
        id: String,
        status: RalphTaskStatus,
        title: String,
        priority: RalphTaskPriority,
        tags: [String] = [],
        createdAt: String? = nil,
        updatedAt: String? = nil,
        agent: RalphTaskAgent? = nil
    ) -> RalphTask {
        RalphTask(
            id: id,
            status: status,
            title: title,
            priority: priority,
            tags: tags,
            agent: agent,
            createdAt: createdAt.flatMap(date(from:)),
            updatedAt: updatedAt.flatMap(date(from:))
        )
    }

    @discardableResult
    static func writeJSONDocument<T: Encodable>(_ value: T, to url: URL) throws -> URL {
        let encoder = JSONEncoder()
        encoder.dateEncodingStrategy = .iso8601
        encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
        let data = try encoder.encode(value)
        try data.write(to: url)
        return url
    }

    @discardableResult
    static func writeJSONDocument<T: Encodable>(
        _ value: T,
        in directory: URL,
        name: String
    ) throws -> URL {
        let url = directory.appendingPathComponent(name, isDirectory: false)
        return try writeJSONDocument(value, to: url)
    }

    static func writeQueueFile(in workspaceURL: URL, tasks: [RalphTask]) throws {
        let ralphDirectoryURL = workspaceURL.appendingPathComponent(".ralph", isDirectory: true)
        try FileManager.default.createDirectory(at: ralphDirectoryURL, withIntermediateDirectories: true)
        try writeJSONDocument(
            RalphTaskQueueDocument(tasks: tasks),
            to: ralphDirectoryURL.appendingPathComponent("queue.jsonc", isDirectory: false)
        )
    }

    static func writeDoneFile(in workspaceURL: URL, tasks: [RalphTask]) throws {
        let ralphDirectoryURL = workspaceURL.appendingPathComponent(".ralph", isDirectory: true)
        try FileManager.default.createDirectory(at: ralphDirectoryURL, withIntermediateDirectories: true)
        try writeJSONDocument(
            RalphTaskQueueDocument(tasks: tasks),
            to: ralphDirectoryURL.appendingPathComponent("done.jsonc", isDirectory: false)
        )
    }

    private static func date(from iso8601: String) -> Date? {
        let formatter = ISO8601DateFormatter()
        return formatter.date(from: iso8601)
    }
}
