/**
 WorkspaceRoutingContractRunner

 Purpose:
 - Run the noninteractive macOS workspace-routing contract inside the app process.

 Responsibilities:
 - Run the noninteractive macOS workspace-routing contract inside the app process.
 - Verify bootstrap window routing, URL-open retarget/focus behavior, and pending scene-route delivery.
 - Write a machine-readable report for `scripts/macos-workspace-routing-contract.sh` and exit explicitly.

 Does not handle:
 - Interactive UI automation.
 - Settings window verification.
 - General app launch policy outside workspace-routing contract mode.

 Usage:
 - Used by the RalphMac app or RalphCore tests through its owning feature surface.

 Invariants/assumptions callers must respect:
 - Contract mode is enabled only via `--workspace-routing-contract`.
 - The script provides disposable workspace A/B/C paths and a report path via environment variables.
 - The seeded workspace C queue contains task `RQ-0300` for pending route verification.
 */

import Darwin
import Foundation
import RalphCore

@MainActor
private struct WorkspaceRoutingContractStepReport: Codable {
    let name: String
    let snapshot: WorkspaceContractDiagnosticsSnapshot
}

private struct WorkspaceRoutingContractReport: Codable {
    let passed: Bool
    let runtimeMode: String
    let workspaceAPath: String
    let workspaceBPath: String
    let workspaceCPath: String
    let steps: [WorkspaceRoutingContractStepReport]
    let failureMessage: String?
}

private struct WorkspaceRoutingContractFailure: LocalizedError {
    let message: String

    var errorDescription: String? {
        message
    }
}

@MainActor
final class WorkspaceRoutingContractRunner {
    static let shared = WorkspaceRoutingContractRunner()

    private enum EnvironmentKey {
        static let workspaceA = "RALPH_WORKSPACE_ROUTING_CONTRACT_WORKSPACE_A"
        static let workspaceB = "RALPH_WORKSPACE_ROUTING_CONTRACT_WORKSPACE_B"
        static let workspaceC = "RALPH_WORKSPACE_ROUTING_CONTRACT_WORKSPACE_C"
        static let reportPath = "RALPH_WORKSPACE_ROUTING_CONTRACT_REPORT_PATH"
    }

    private struct Configuration {
        let workspaceAURL: URL
        let workspaceBURL: URL
        let workspaceCURL: URL
        let reportURL: URL

        var workspaceAPath: String { Self.normalizedPath(workspaceAURL) }
        var workspaceBPath: String { Self.normalizedPath(workspaceBURL) }
        var workspaceCPath: String { Self.normalizedPath(workspaceCURL) }

        static func loadFromEnvironment(
            _ environment: [String: String] = ProcessInfo.processInfo.environment
        ) throws -> Self {
            func requiredURL(_ key: String, directory: Bool) throws -> URL {
                guard let rawValue = environment[key]?.trimmingCharacters(in: .whitespacesAndNewlines),
                      !rawValue.isEmpty else {
                    throw WorkspaceRoutingContractFailure(message: "Missing required environment variable: \(key)")
                }
                return URL(fileURLWithPath: rawValue, isDirectory: directory)
                    .standardizedFileURL
                    .resolvingSymlinksInPath()
            }

            return Configuration(
                workspaceAURL: try requiredURL(EnvironmentKey.workspaceA, directory: true),
                workspaceBURL: try requiredURL(EnvironmentKey.workspaceB, directory: true),
                workspaceCURL: try requiredURL(EnvironmentKey.workspaceC, directory: true),
                reportURL: try requiredURL(EnvironmentKey.reportPath, directory: false)
            )
        }

        private static func normalizedPath(_ url: URL) -> String {
            url.standardizedFileURL.resolvingSymlinksInPath().path
        }
    }

    private var contractTask: Task<Void, Never>?

    private init() {}

    func configureIfNeeded() {
        guard RalphAppDefaults.isWorkspaceRoutingContract else { return }
        guard contractTask == nil else { return }

        contractTask = Task { @MainActor [weak self] in
            guard let self else { return }
            await self.runAndExit()
        }
    }

    private func runAndExit() async {
        let configuration: Configuration
        do {
            configuration = try Configuration.loadFromEnvironment()
        } catch {
            fputs("Workspace routing contract misconfigured: \(error.localizedDescription)\n", stderr)
            Darwin.exit(1)
        }

        do {
            let report = try await runContract(configuration)
            try writeReport(report, to: configuration.reportURL)
            print("Workspace routing contract passed.")
            Darwin.exit(0)
        } catch {
            let failureReport = WorkspaceRoutingContractReport(
                passed: false,
                runtimeMode: "workspace-routing-contract",
                workspaceAPath: configuration.workspaceAPath,
                workspaceBPath: configuration.workspaceBPath,
                workspaceCPath: configuration.workspaceCPath,
                steps: [],
                failureMessage: error.localizedDescription
            )
            try? writeReport(failureReport, to: configuration.reportURL)
            fputs("Workspace routing contract failed: \(error.localizedDescription)\n", stderr)
            Darwin.exit(1)
        }
    }

    private func runContract(_ configuration: Configuration) async throws -> WorkspaceRoutingContractReport {
        var steps: [WorkspaceRoutingContractStepReport] = []

        let bootstrapSnapshot = try await waitForSnapshot(
            stepName: "initial-bootstrap",
            expectedWorkspacePath: configuration.workspaceAPath,
            expectedTaskCount: 1,
            expectedWorkspaceCount: 1,
            expectedSelectedTaskID: nil,
            expectedSelectedSection: nil,
            expectedVisibleWorkspaceWindowCount: 1,
            expectedPlaceholder: true
        )
        steps.append(WorkspaceRoutingContractStepReport(name: "initial-bootstrap", snapshot: bootstrapSnapshot))

        RalphURLRouter.handle(workspaceOpenURL(for: configuration.workspaceBURL))
        let bootstrapRetargetSnapshot = try await waitForSnapshot(
            stepName: "url-open-bootstrap-retarget",
            expectedWorkspacePath: configuration.workspaceBPath,
            expectedTaskCount: 1,
            expectedWorkspaceCount: 1,
            expectedSelectedTaskID: nil,
            expectedSelectedSection: nil,
            expectedVisibleWorkspaceWindowCount: 1,
            expectedPlaceholder: false
        )
        steps.append(WorkspaceRoutingContractStepReport(name: "url-open-bootstrap-retarget", snapshot: bootstrapRetargetSnapshot))

        let appendedWorkspace = WorkspaceManager.shared.createWorkspace(workingDirectory: configuration.workspaceCURL)
        WorkspaceManager.shared.route(.showTaskDetail(taskID: "RQ-0300"), to: appendedWorkspace.id)
        let pendingRouteSnapshot = try await waitForSnapshot(
            stepName: "route-pending-task-detail-to-new-workspace",
            expectedWorkspacePath: configuration.workspaceCPath,
            expectedTaskCount: 1,
            expectedWorkspaceCount: 2,
            expectedSelectedTaskID: "RQ-0300",
            expectedSelectedSection: SidebarSection.queue.rawValue,
            expectedVisibleWorkspaceWindowCount: 1,
            expectedPlaceholder: false
        )
        steps.append(WorkspaceRoutingContractStepReport(name: "route-pending-task-detail-to-new-workspace", snapshot: pendingRouteSnapshot))

        RalphURLRouter.handle(workspaceOpenURL(for: configuration.workspaceBURL))
        let existingWorkspaceSnapshot = try await waitForSnapshot(
            stepName: "url-open-existing-workspace-focus",
            expectedWorkspacePath: configuration.workspaceBPath,
            expectedTaskCount: 1,
            expectedWorkspaceCount: 2,
            expectedSelectedTaskID: nil,
            expectedSelectedSection: nil,
            expectedVisibleWorkspaceWindowCount: 1,
            expectedPlaceholder: false,
            extraValidation: {
                let manager = WorkspaceManager.shared
                let workspaceBMatches = manager.workspaces.filter { $0.matchesWorkingDirectory(configuration.workspaceBURL) }
                return workspaceBMatches.count == 1
                    ? nil
                    : "workspaceB instance count=\(workspaceBMatches.count) expected 1"
            }
        )
        steps.append(WorkspaceRoutingContractStepReport(name: "url-open-existing-workspace-focus", snapshot: existingWorkspaceSnapshot))

        return WorkspaceRoutingContractReport(
            passed: true,
            runtimeMode: "workspace-routing-contract",
            workspaceAPath: configuration.workspaceAPath,
            workspaceBPath: configuration.workspaceBPath,
            workspaceCPath: configuration.workspaceCPath,
            steps: steps,
            failureMessage: nil
        )
    }

    private func waitForSnapshot(
        stepName: String,
        expectedWorkspacePath: String,
        expectedTaskCount: Int,
        expectedWorkspaceCount: Int,
        expectedSelectedTaskID: String?,
        expectedSelectedSection: String?,
        expectedVisibleWorkspaceWindowCount: Int,
        expectedPlaceholder: Bool,
        extraValidation: @MainActor @escaping () -> String? = { nil }
    ) async throws -> WorkspaceContractDiagnosticsSnapshot {
        let deadline = Date().addingTimeInterval(25)
        var lastSnapshot = WorkspaceContractPresentationCoordinator.shared.diagnostics
        var lastFailures = ["snapshot not captured yet"]

        while Date() < deadline {
            lastSnapshot = WorkspaceContractPresentationCoordinator.shared.diagnostics
            lastFailures = snapshotFailures(
                lastSnapshot,
                expectedWorkspacePath: expectedWorkspacePath,
                expectedTaskCount: expectedTaskCount,
                expectedWorkspaceCount: expectedWorkspaceCount,
                expectedSelectedTaskID: expectedSelectedTaskID,
                expectedSelectedSection: expectedSelectedSection,
                expectedVisibleWorkspaceWindowCount: expectedVisibleWorkspaceWindowCount,
                expectedPlaceholder: expectedPlaceholder
            )
            if let extraFailure = extraValidation() {
                lastFailures.append(extraFailure)
            }
            if lastFailures.isEmpty {
                return lastSnapshot
            }
            try? await Task.sleep(nanoseconds: 50_000_000)
        }

        throw WorkspaceRoutingContractFailure(
            message: "Timed out waiting for \(stepName). Failures: \(lastFailures.joined(separator: "; ")). Last snapshot: \(encodedSnapshot(lastSnapshot))"
        )
    }

    private func snapshotFailures(
        _ snapshot: WorkspaceContractDiagnosticsSnapshot,
        expectedWorkspacePath: String,
        expectedTaskCount: Int,
        expectedWorkspaceCount: Int,
        expectedSelectedTaskID: String?,
        expectedSelectedSection: String?,
        expectedVisibleWorkspaceWindowCount: Int,
        expectedPlaceholder: Bool
    ) -> [String] {
        var failures: [String] = []

        if Self.normalizedPath(snapshot.workspacePath) != expectedWorkspacePath {
            failures.append("workspacePath=\(snapshot.workspacePath ?? "nil") expected \(expectedWorkspacePath)")
        }
        if snapshot.taskCount != expectedTaskCount {
            failures.append("taskCount=\(snapshot.taskCount) expected \(expectedTaskCount)")
        }
        if snapshot.workspaceCount != expectedWorkspaceCount {
            failures.append("workspaceCount=\(snapshot.workspaceCount) expected \(expectedWorkspaceCount)")
        }
        if snapshot.visibleWorkspaceWindowCount != expectedVisibleWorkspaceWindowCount {
            failures.append(
                "visibleWorkspaceWindowCount=\(snapshot.visibleWorkspaceWindowCount) expected \(expectedVisibleWorkspaceWindowCount)"
            )
        }
        if snapshot.tasksLoading {
            failures.append("tasksLoading should be false")
        }
        if snapshot.tasksErrorMessage != nil {
            failures.append("tasksErrorMessage should be nil (got \(snapshot.tasksErrorMessage ?? "nil"))")
        }
        if snapshot.isPlaceholder != expectedPlaceholder {
            failures.append("isPlaceholder=\(snapshot.isPlaceholder) expected \(expectedPlaceholder)")
        }
        if let expectedSelectedTaskID {
            if snapshot.selectedTaskID != expectedSelectedTaskID {
                failures.append("selectedTaskID=\(snapshot.selectedTaskID ?? "nil") expected \(expectedSelectedTaskID)")
            }
        } else if snapshot.selectedTaskID != nil {
            failures.append("selectedTaskID=\(snapshot.selectedTaskID ?? "nil") expected nil")
        }
        if let expectedSelectedSection, snapshot.selectedSection != expectedSelectedSection {
            failures.append("selectedSection=\(snapshot.selectedSection ?? "nil") expected \(expectedSelectedSection)")
        }

        return failures
    }

    private func workspaceOpenURL(for workspaceURL: URL) -> URL {
        var components = URLComponents()
        components.scheme = "ralph"
        components.host = "open"
        components.queryItems = [
            URLQueryItem(name: "workspace", value: workspaceURL.path)
        ]
        return components.url!
    }

    private func writeReport(_ report: WorkspaceRoutingContractReport, to url: URL) throws {
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
        let data = try encoder.encode(report)
        try FileManager.default.createDirectory(
            at: url.deletingLastPathComponent(),
            withIntermediateDirectories: true,
            attributes: nil
        )
        try data.write(to: url, options: .atomic)
    }

    private func encodedSnapshot(_ snapshot: WorkspaceContractDiagnosticsSnapshot) -> String {
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.sortedKeys]
        guard let data = try? encoder.encode(snapshot),
              let string = String(data: data, encoding: .utf8) else {
            return "{}"
        }
        return string
    }

    private static func normalizedPath(_ path: String?) -> String? {
        guard let path, !path.isEmpty else { return nil }
        return URL(fileURLWithPath: path, isDirectory: true)
            .standardizedFileURL
            .resolvingSymlinksInPath()
            .path
    }
}
