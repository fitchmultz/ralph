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
            return "Queue validation skipped\n\nNo `.ralph/queue.jsonc` found in \(workspace.workingDirectoryURL.path).\nRun `ralph init --non-interactive` in this directory first."
        }

        do {
            let client: RalphCLIClient
            if let managerClient = WorkspaceManager.shared.client {
                client = managerClient
            } else {
                client = try RalphCLIClient.bundled()
            }

            let result = try await client.runAndCollect(
                arguments: ["--no-color", "queue", "validate"],
                currentDirectoryURL: workspace.workingDirectoryURL
            )

            if result.status.code == 0 {
                let stdout = result.stdout.trimmingCharacters(in: .whitespacesAndNewlines)
                if stdout.isEmpty {
                    return "Queue validation passed."
                }
                return "Queue validation passed.\n\n\(stdout)"
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

        return await withCheckedContinuation { continuation in
            RalphLogger.shared.exportLogs(hours: hours) { logs in
                continuation.resume(returning: logs ?? "No logs available")
            }
        }
    }
}
