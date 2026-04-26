/**
 WorkspaceDiagnosticsService

 Purpose:
 - Execute workspace-scoped diagnostics commands used by recovery UI.

 Responsibilities:
 - Execute workspace-scoped diagnostics commands used by recovery UI.
 - Load recent Ralph logs through the shared logger using async-friendly APIs.
 - Keep diagnostics and recovery command orchestration out of SwiftUI views.
 - Format continuation documents into human-readable recovery summaries.

 Does not handle:
 - SwiftUI sheet presentation or button state.
 - Error classification.
 - Opening Finder, links, or pasteboard integration.

 Usage:
 - Used by the RalphMac app or RalphCore tests through its owning feature surface.

 Invariants/assumptions callers must respect:
 - Diagnostics run against a live `Workspace` configured on the main actor.
 - Queue validation requires a Ralph queue file in the workspace.
 - Log export may be unavailable on older macOS runtimes.
 */

import Foundation

public struct QueueLockDiagnosticSnapshot: Equatable, Sendable {
    public enum Condition: String, Equatable, Sendable {
        case clear
        case live
        case stale
        case ownerMissing
        case ownerUnreadable
        case unknown

        public var displayName: String {
            switch self {
            case .clear: return "Clear"
            case .live: return "Live Holder"
            case .stale: return "Stale Lock"
            case .ownerMissing: return "Owner Metadata Missing"
            case .ownerUnreadable: return "Owner Metadata Unreadable"
            case .unknown: return "Unknown"
            }
        }
    }

    public let condition: Condition
    let blocking: WorkspaceRunnerController.MachineBlockingState?
    let doctorOutput: String
    public let unlockPreview: String
    public let unlockAllowed: Bool

    public var canClearStaleLock: Bool {
        condition == .stale && unlockAllowed
    }
}

@MainActor
public enum WorkspaceDiagnosticsService {
    public static func queueValidationOutput(for workspace: Workspace) async -> String {
        guard let client = workspace.client else {
            return "Queue validation skipped\n\nCLI client not available."
        }

        switch await workspace.ensureQueueAccessPreflight(
            client: client,
            retryConfiguration: .minimal
        ) {
        case .ready:
            break
        case .configResolutionFailed(let recoveryError):
            return """
            Queue validation skipped

            RalphMac could not resolve the workspace queue paths before validation.

            \(recoveryError.message)
            """
        case .missingConfiguredQueueFile(let queueURL):
            return """
            Queue validation skipped

            No queue file was found at the configured path:
            \(queueURL.path)

            Confirm the active Ralph queue configuration or inspect `ralph machine config resolve`.
            """
        }

        do {
            let document = try await workspace.validateQueueContinuation()
            return formatQueueValidation(document)
        } catch {
            return "Failed to run queue validation: \(error.localizedDescription)"
        }
    }

    public static func queueRepairPreviewOutput(for workspace: Workspace) async -> String {
        do {
            let document = try await workspace.repairQueueContinuation(dryRun: true)
            return formatContinuationDocument(
                headline: document.continuation.headline,
                detail: document.continuation.detail,
                blocking: document.effectiveBlocking,
                nextSteps: document.continuation.nextSteps,
                body: document.report.prettyPrintedString ?? "No repair report payload was returned."
            )
        } catch {
            return "Failed to preview queue repair: \(error.localizedDescription)"
        }
    }

    public static func queueRestorePreviewOutput(for workspace: Workspace) async -> String {
        do {
            let document = try await workspace.restoreQueueContinuation(dryRun: true)
            let body = document.result?.prettyPrintedString ?? "No restore preview payload was returned."
            return formatContinuationDocument(
                headline: document.continuation.headline,
                detail: document.continuation.detail,
                blocking: document.effectiveBlocking,
                nextSteps: document.continuation.nextSteps,
                body: body
            )
        } catch {
            return "Failed to preview queue restore: \(error.localizedDescription)"
        }
    }

    public static func recentLogs(hours: Int = 2) async -> String {
        guard RalphLogger.shared.canExportLogs else {
            return "Log export requires macOS 12.0+"
        }

        do {
            return try await RalphLogger.shared.exportLogs(hours: hours)
        } catch {
            return "Failed to export logs: \(error.localizedDescription)"
        }
    }

    public static func queueLockDiagnosticSnapshot(
        for workspace: Workspace
    ) async -> QueueLockDiagnosticSnapshot {
        do {
            let doctorDocument = try await machineDoctorReport(for: workspace)
            let unlockInspect = try await queueUnlockInspect(for: workspace)
            return QueueLockDiagnosticSnapshot(
                condition: deriveQueueLockCondition(from: unlockInspect.condition),
                blocking: unlockInspect.blocking ?? doctorDocument.blocking,
                doctorOutput: formatDoctorReport(doctorDocument),
                unlockPreview: formatQueueUnlockInspect(unlockInspect),
                unlockAllowed: unlockInspect.unlockAllowed
            )
        } catch {
            return QueueLockDiagnosticSnapshot(
                condition: .unknown,
                blocking: nil,
                doctorOutput: "Failed to inspect queue lock: \(error.localizedDescription)",
                unlockPreview: "Failed to preview queue unlock: \(error.localizedDescription)",
                unlockAllowed: false
            )
        }
    }

    public static func queueLockInspectionOutput(for workspace: Workspace) async -> String {
        let snapshot = await queueLockDiagnosticSnapshot(for: workspace)
        return formatQueueLockSnapshot(snapshot)
    }

    public static func queueUnlockPreviewOutput(for workspace: Workspace) async -> String {
        let snapshot = await queueLockDiagnosticSnapshot(for: workspace)
        return snapshot.unlockPreview
    }

    public static func clearStaleQueueLock(for workspace: Workspace) async -> String {
        let snapshot = await queueLockDiagnosticSnapshot(for: workspace)
        guard snapshot.canClearStaleLock else {
            return """
            Queue lock will not be cleared.

            Current condition: \(snapshot.condition.displayName)

            \(snapshot.unlockPreview)
            """
        }

        do {
            let output = try await runCollectedCommand(
                workspace: workspace,
                arguments: ["queue", "unlock", "--yes"],
                timeoutConfiguration: .default
            )
            if output.status.code != 0 {
                let message = output.stderr.trimmingCharacters(in: .whitespacesAndNewlines)
                return message.isEmpty
                    ? "Failed to clear the stale queue lock (exit \(output.status.code))."
                    : message
            }
            await workspace.refreshRunControlStatusData()
            return output.stdout.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
                ? "Stale queue lock cleared."
                : output.stdout
        } catch {
            return "Failed to clear stale queue lock: \(error.localizedDescription)"
        }
    }

    private static func formatQueueValidation(_ document: MachineQueueValidateDocument) -> String {
        var sections: [String] = [document.continuation.headline, "", document.continuation.detail]

        if let blocking = document.effectiveBlocking {
            sections.append("")
            sections.append("Operator state: \(blocking.status.rawValue)")
            sections.append(blocking.message)
            if !blocking.detail.isEmpty {
                sections.append(blocking.detail)
            }
        }

        if !document.warnings.isEmpty {
            sections.append("")
            sections.append("Warnings:")
            sections.append(contentsOf: document.warnings.map { "- [\($0.taskID)] \($0.message)" })
        }

        if !document.continuation.nextSteps.isEmpty {
            sections.append("")
            sections.append("Next:")
            sections.append(
                contentsOf: document.continuation.nextSteps.enumerated().map { index, step in
                    "\(index + 1). \(step.command) — \(step.detail)"
                }
            )
        }

        return sections.joined(separator: "\n")
    }

    private static func formatContinuationDocument(
        headline: String,
        detail: String,
        blocking: WorkspaceRunnerController.MachineBlockingState?,
        nextSteps: [WorkspaceContinuationAction],
        body: String
    ) -> String {
        var sections: [String] = [headline, "", detail]

        if let blocking {
            sections.append("")
            sections.append("Operator state: \(blocking.status.rawValue)")
            sections.append(blocking.message)
            if !blocking.detail.isEmpty {
                sections.append(blocking.detail)
            }
        }

        if !body.isEmpty {
            sections.append("")
            sections.append(body)
        }

        if !nextSteps.isEmpty {
            sections.append("")
            sections.append("Next:")
            sections.append(
                contentsOf: nextSteps.enumerated().map { index, step in
                    "\(index + 1). \(step.command) — \(step.detail)"
                }
            )
        }

        return sections.joined(separator: "\n")
    }

    private static func formatDoctorReport(_ document: MachineDoctorReportDocument) -> String {
        var sections: [String] = []
        if let blocking = document.blocking {
            sections.append("Operator state: \(blocking.status.rawValue)")
            sections.append(blocking.message)
            if !blocking.detail.isEmpty {
                sections.append(blocking.detail)
            }
            sections.append("")
        }
        sections.append(document.report.prettyPrintedString ?? "No doctor report payload was returned.")
        return sections.joined(separator: "\n")
    }

    private static func formatQueueLockSnapshot(_ snapshot: QueueLockDiagnosticSnapshot) -> String {
        var sections: [String] = [
            "Queue Lock Inspection",
            "",
            "Condition: \(snapshot.condition.displayName)"
        ]
        if let blocking = snapshot.blocking {
            sections.append("Operator state: \(blocking.status.rawValue)")
            sections.append(blocking.message)
            if !blocking.detail.isEmpty {
                sections.append(blocking.detail)
            }
        }
        sections.append("")
        sections.append("Doctor Report")
        sections.append(snapshot.doctorOutput)
        sections.append("")
        sections.append("Unlock Preview")
        sections.append(snapshot.unlockPreview)
        return sections.joined(separator: "\n")
    }

    private static func deriveQueueLockCondition(
        from condition: MachineQueueUnlockInspectDocument.Condition
    ) -> QueueLockDiagnosticSnapshot.Condition {
        switch condition {
        case .clear: return .clear
        case .live: return .live
        case .stale: return .stale
        case .ownerMissing: return .ownerMissing
        case .ownerUnreadable: return .ownerUnreadable
        }
    }

    private static func machineDoctorReport(
        for workspace: Workspace
    ) async throws -> MachineDoctorReportDocument {
        guard let client = workspace.client else {
            throw Workspace.WorkspaceError.cliClientUnavailable
        }
        return try await workspace.decodeMachineRepositoryJSON(
            MachineDoctorReportDocument.self,
            client: client,
            machineArguments: ["doctor", "report"],
            currentDirectoryURL: workspace.identityState.workingDirectoryURL,
            retryConfiguration: .minimal,
            onRetry: nil
        )
    }

    private static func queueUnlockInspect(for workspace: Workspace) async throws -> MachineQueueUnlockInspectDocument {
        guard let client = workspace.client else {
            throw Workspace.WorkspaceError.cliClientUnavailable
        }
        return try await workspace.decodeMachineRepositoryJSON(
            MachineQueueUnlockInspectDocument.self,
            client: client,
            machineArguments: ["queue", "unlock-inspect"],
            currentDirectoryURL: workspace.identityState.workingDirectoryURL,
            retryConfiguration: .minimal,
            onRetry: nil
        )
    }

    private static func formatQueueUnlockInspect(_ document: MachineQueueUnlockInspectDocument) -> String {
        var sections = [document.continuation.headline, "", document.continuation.detail]
        if let blocking = document.blocking {
            sections.append("")
            sections.append("Operator state: \(blocking.status.rawValue)")
            sections.append(blocking.message)
            if !blocking.detail.isEmpty {
                sections.append(blocking.detail)
            }
        }
        sections.append("")
        sections.append("Unlock allowed: \(document.unlockAllowed ? "yes" : "no")")
        return sections.joined(separator: "\n")
    }

    private static func runCollectedCommand(
        workspace: Workspace,
        arguments: [String],
        timeoutConfiguration: TimeoutConfiguration
    ) async throws -> RalphCLIClient.CollectedOutput {
        guard let client = workspace.client else {
            throw Workspace.WorkspaceError.cliClientUnavailable
        }

        let helper = RetryHelper(configuration: .minimal)
        return try await helper.execute {
            try await client.runAndCollect(
                arguments: arguments,
                currentDirectoryURL: workspace.identityState.workingDirectoryURL,
                timeoutConfiguration: timeoutConfiguration
            )
        }
    }
}

private extension RalphJSONValue {
    var prettyPrintedString: String? {
        guard let data = try? JSONEncoder().encode(self),
              let object = try? JSONSerialization.jsonObject(with: data),
              let prettyData = try? JSONSerialization.data(withJSONObject: object, options: [.prettyPrinted]),
              let string = String(data: prettyData, encoding: .utf8)
        else {
            return nil
        }
        return string
    }
}
