/**
 WorkspaceDiagnosticsService

 Responsibilities:
 - Execute workspace-scoped diagnostics commands used by recovery UI.
 - Load recent Ralph logs through the shared logger using async-friendly APIs.
 - Keep diagnostics/recovery command orchestration out of SwiftUI views.

 Does not handle:
 - SwiftUI sheet presentation or button state.
 - Error classification.
 - Opening Finder, links, or pasteboard integration.

 Invariants/assumptions callers must respect:
 - Diagnostics run against a live `Workspace` configured on the main actor.
 - Queue validation requires a Ralph queue file in the workspace.
 - Log export may be unavailable on older macOS runtimes.
 */

import Foundation

@MainActor
public enum WorkspaceDiagnosticsService {
    public static func queueValidationOutput(for workspace: Workspace) async -> String {
        guard workspace.hasRalphQueueFile else {
            return "Queue validation skipped\n\nNo `.ralph/queue.jsonc` found in \(workspace.identityState.workingDirectoryURL.path).\nRun `ralph init --non-interactive` in this directory first."
        }

        do {
            let client: RalphCLIClient
            if let managerClient = WorkspaceManager.shared.client {
                client = managerClient
            } else {
                client = try RalphCLIClient.bundled()
            }

            let result = try await client.runAndCollect(
                arguments: ["--no-color", "machine", "queue", "validate"],
                currentDirectoryURL: workspace.identityState.workingDirectoryURL
            )

            if result.status.code == 0 {
                let decoder = JSONDecoder()
                let document = try decoder.decode(MachineQueueValidationDocument.self, from: Data(result.stdout.utf8))
                if document.warnings.isEmpty {
                    return "Queue validation passed."
                }
                let warningLines = document.warnings.map { "- [\($0.taskID)] \($0.message)" }.joined(separator: "\n")
                return "Queue validation passed with warnings.\n\n\(warningLines)"
            }

            let stderr = result.stderr.trimmingCharacters(in: .whitespacesAndNewlines)
            if stderr.isEmpty {
                return "Queue validation failed.\n\nExit code: \(result.status.code)"
            }
            return "Queue validation failed.\n\nExit code: \(result.status.code)\n\(stderr)"
        } catch {
            return "Failed to run queue validation: \(error.localizedDescription)"
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
}

private struct MachineQueueValidationDocument: Decodable {
    let version: Int
    let valid: Bool
    let warnings: [Warning]

    struct Warning: Decodable {
        let taskID: String
        let message: String

        enum CodingKeys: String, CodingKey {
            case taskID = "task_id"
            case message
        }
    }
}
