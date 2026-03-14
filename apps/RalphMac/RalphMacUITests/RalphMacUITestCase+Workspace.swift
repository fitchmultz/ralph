/**
 Purpose:
 - Own CLI-backed workspace and process helpers for Ralph macOS UI tests.

 Responsibilities:
 - Create and seed isolated UI-test workspaces.
 - Run the bundled `ralph` executable and decode queue state.
 - Relaunch the app against the same workspace when app-state regressions need it.

 Scope:
 - Filesystem/process helpers only.

 Usage:
 - Base harness and app-state suites call these helpers to prepare or inspect the fixture workspace.

 Invariants/Assumptions:
 - `ralphExecutableURL` is resolved before command helpers run.
 - Seeded fixtures stay stable unless a test intentionally mutates the workspace.
 */

import XCTest

@MainActor
extension RalphMacUITestCase {
    func makeUITestWorkspace() throws -> URL {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent("ralph-ui-tests", isDirectory: true)
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)

        try runRalph(arguments: ["init", "--non-interactive"], currentDirectoryURL: root)
        try seedUITestQueue(at: root)
        return root
    }

    func makeAdditionalUITestWorkspace() throws -> URL {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent("ralph-ui-tests", isDirectory: true)
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        try runRalph(arguments: ["init", "--non-interactive"], currentDirectoryURL: root)
        return root
    }

    func seedUITestQueue(at workspaceURL: URL) throws {
        let importURL = workspaceURL.appendingPathComponent("ui-fixture-import.json", isDirectory: false)
        let seededTasks = #"""
        [
          {
            "id": "RQ-0001",
            "status": "todo",
            "title": "UI Fixture Alpha",
            "priority": "high",
            "tags": ["ui", "fixture"],
            "created_at": "2026-03-05T00:00:00Z",
            "updated_at": "2026-03-05T00:00:00Z"
          },
          {
            "id": "RQ-0002",
            "status": "todo",
            "title": "UI Fixture Search Test",
            "priority": "medium",
            "tags": ["ui", "search"],
            "created_at": "2026-03-05T00:05:00Z",
            "updated_at": "2026-03-05T00:05:00Z"
          }
        ]
        """#
        try seededTasks.write(to: importURL, atomically: true, encoding: .utf8)
        defer { XCTAssertNoThrow(try removeItemIfExists(importURL)) }

        try runRalph(
            arguments: ["queue", "import", "--format", "json", "--input", importURL.path],
            currentDirectoryURL: workspaceURL
        )
    }

    func runRalph(arguments: [String], currentDirectoryURL: URL) throws {
        _ = try runRalphAndCollectOutput(arguments: arguments, currentDirectoryURL: currentDirectoryURL)
    }

    func runRalphAndCollectOutput(arguments: [String], currentDirectoryURL: URL) throws -> String {
        guard let executableURL = ralphExecutableURL else {
            throw NSError(
                domain: "RalphMacUITests",
                code: 1,
                userInfo: [NSLocalizedDescriptionKey: "Failed to resolve a ralph executable for UI tests"]
            )
        }

        let process = Process()
        process.executableURL = executableURL
        process.currentDirectoryURL = currentDirectoryURL
        process.arguments = ["--no-color"] + arguments

        let stdoutPipe = Pipe()
        let stderrPipe = Pipe()
        process.standardOutput = stdoutPipe
        process.standardError = stderrPipe

        try process.run()
        process.waitUntilExit()

        let stdout = String(data: stdoutPipe.fileHandleForReading.readDataToEndOfFile(), encoding: .utf8) ?? ""
        let stderr = String(data: stderrPipe.fileHandleForReading.readDataToEndOfFile(), encoding: .utf8) ?? ""

        guard process.terminationStatus == 0 else {
            throw NSError(
                domain: "RalphMacUITests",
                code: Int(process.terminationStatus),
                userInfo: [
                    NSLocalizedDescriptionKey: "ralph \(arguments.joined(separator: " ")) failed",
                    "stdout": stdout,
                    "stderr": stderr
                ]
            )
        }

        return stdout
    }

    func uiTestWorkspaceTasks() throws -> [UITaskSnapshot] {
        guard let uiTestWorkspaceURL else {
            return []
        }

        let output = try runRalphAndCollectOutput(
            arguments: ["queue", "list", "--format", "json"],
            currentDirectoryURL: uiTestWorkspaceURL
        )
        return try JSONDecoder().decode([UITaskSnapshot].self, from: Data(output.utf8))
    }

    func resolveRalphExecutableURL(
        environment: [String: String] = ProcessInfo.processInfo.environment
    ) throws -> URL {
        if let override = environment[LaunchEnvironment.ralphBinPath]?
            .trimmingCharacters(in: .whitespacesAndNewlines),
           !override.isEmpty {
            let overrideURL = URL(fileURLWithPath: override, isDirectory: false)
                .standardizedFileURL
                .resolvingSymlinksInPath()
            guard FileManager.default.isExecutableFile(atPath: overrideURL.path) else {
                throw NSError(
                    domain: "RalphMacUITests",
                    code: 2,
                    userInfo: [
                        NSLocalizedDescriptionKey: "RALPH_BIN_PATH points to a non-executable path: \(overrideURL.path)"
                    ]
                )
            }
            return overrideURL
        }

        let bundledURL = Bundle.main.bundleURL
            .deletingLastPathComponent()
            .appendingPathComponent("RalphMac.app", isDirectory: true)
            .appendingPathComponent("Contents", isDirectory: true)
            .appendingPathComponent("MacOS", isDirectory: true)
            .appendingPathComponent("ralph", isDirectory: false)
            .standardizedFileURL
            .resolvingSymlinksInPath()
        if FileManager.default.isExecutableFile(atPath: bundledURL.path) {
            return bundledURL
        }

        throw NSError(
            domain: "RalphMacUITests",
            code: 2,
            userInfo: [
                NSLocalizedDescriptionKey: "Failed to locate a bundled ralph executable for UI tests at \(bundledURL.path). Build the app bundle or set RALPH_BIN_PATH explicitly."
            ]
        )
    }

    func relaunchApp() {
        stopTimelineCapture()
        app.terminate()
        app.launch()
        app.activate()
        startTimelineCaptureIfNeeded()
    }

    func openWorkspaceURLInApp(_ workspaceURL: URL) throws {
        DistributedNotificationCenter.default().postNotificationName(
            Notification.Name("com.mitchfultz.ralph.uitesting.openWorkspace"),
            object: nil,
            userInfo: ["workspacePath": workspaceURL.path],
            deliverImmediately: true
        )
    }

    func removeItemIfExists(_ url: URL) throws {
        guard FileManager.default.fileExists(atPath: url.path) else {
            return
        }
        try FileManager.default.removeItem(at: url)
    }
}
