/**
 MenuBarIconView

 Responsibilities:
 - Render the menu bar icon with status indication.
 - Show different icons based on workspace state (idle, running, all done).
 - Support both MenuBarExtra and NSStatusBar contexts.

 Does not handle:
 - Menu content rendering.
 - Business logic.

 Invariants/assumptions:
 - Observes WorkspaceManager.shared for state changes.
 - Must run on MainActor.
 */

import SwiftUI
import RalphCore

/// View for the menu bar icon that changes based on workspace state.
struct MenuBarIconView: View {
    @ObservedObject private var manager = WorkspaceManager.shared
    
    var body: some View {
        if let workspace = manager.workspaces.first {
            icon(for: workspace)
        } else {
            // Default icon when no workspace is available
            Image(systemName: "checklist")
                .foregroundStyle(.primary)
        }
    }
    
    /// Determine the appropriate icon based on workspace state
    private func icon(for workspace: Workspace) -> some View {
        let iconName: String
        let color: Color
        
        if workspace.runState.isRunning {
            // Running state - animated checklist or RTL version
            iconName = "checklist.rtl"
            color = .orange
        } else if workspace.nextTask() != nil {
            // Has pending tasks
            iconName = "checklist"
            color = .primary
        } else if !workspace.taskState.tasks.isEmpty {
            // All tasks done (tasks exist but none are todo)
            iconName = "checkmark.square.fill"
            color = .green
        } else {
            // No tasks at all
            iconName = "checklist"
            color = .secondary
        }
        
        return Image(systemName: iconName)
            .foregroundStyle(color)
    }
}

#Preview {
    MenuBarIconView()
}
