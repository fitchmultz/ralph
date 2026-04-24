/**
 WorkspaceView+Diagnostics

 Purpose:
 - Render workspace state probes used by noninteractive UI contract tests.

 Responsibilities:
 - Render workspace state probes used by noninteractive UI contract tests.
 - Provide the shared empty-detail placeholder used by workspace sections.

 Does not handle:
 - Command routing.
 - Task detail editing.

 Usage:
 - Used by the RalphMac app or RalphCore tests through its owning feature surface.

 Invariants/Assumptions:
 - Callers keep usage within the documented responsibilities and owning feature contracts.
 */

import RalphCore
import SwiftUI

struct WorkspaceStateAccessibilityProbe: View {
    @ObservedObject var workspace: Workspace

    private struct Snapshot: Encodable {
        let workspaceID: String
        let workspacePath: String
        let projectDisplayName: String
        let taskCount: Int
        let tasksLoading: Bool
        let tasksErrorMessage: String?
        let isPlaceholder: Bool
        let retargetRevision: UInt64
        let workspaceCount: Int
        let focusedWorkspaceID: String?
        let effectiveWorkspaceID: String?
    }

    private var encodedSnapshot: String {
        let manager = WorkspaceManager.shared
        let snapshot = Snapshot(
            workspaceID: workspace.id.uuidString,
            workspacePath: workspace.identityState.workingDirectoryURL.path,
            projectDisplayName: workspace.projectDisplayName,
            taskCount: workspace.taskState.tasks.count,
            tasksLoading: workspace.taskState.tasksLoading,
            tasksErrorMessage: workspace.taskState.tasksErrorMessage,
            isPlaceholder: workspace.isURLRoutingPlaceholderWorkspace,
            retargetRevision: workspace.identityState.retargetRevision,
            workspaceCount: manager.workspaces.count,
            focusedWorkspaceID: manager.focusedWorkspace?.id.uuidString,
            effectiveWorkspaceID: manager.effectiveWorkspace?.id.uuidString
        )

        guard let data = try? JSONEncoder().encode(snapshot),
              let json = String(data: data, encoding: .utf8) else {
            return "{}"
        }
        return json
    }

    var body: some View {
        Text(encodedSnapshot)
            .font(.system(size: 1))
            .foregroundStyle(.clear)
            .frame(width: 1, height: 1)
            .clipped()
            .allowsHitTesting(false)
            .accessibilityIdentifier("workspace-state-probe")
    }
}

@MainActor
struct EmptyDetailView: View {
    let icon: String
    let title: String
    let message: String

    var body: some View {
        VStack(spacing: 16) {
            Image(systemName: icon)
                .font(.system(size: 48))
                .foregroundStyle(.secondary)
                .accessibilityHidden(true)

            Text(title)
                .font(.headline)

            Text(message)
                .font(.subheadline)
                .foregroundStyle(.secondary)
                .multilineTextAlignment(.center)
                .frame(maxWidth: 300)
        }
        .accessibilityElement(children: .combine)
        .accessibilityLabel("\(title). \(message)")
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(.clear)
    }
}
