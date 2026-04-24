/**
 WorkspaceView+Sheets

 Purpose:
 - Build workspace-scoped sheet content for error recovery, command palette, and operational health.

 Responsibilities:
 - Build workspace-scoped sheet content for error recovery, command palette, and operational health.
 - Keep sheet composition out of the root split-view shell.

 Does not handle:
 - Sheet trigger actions.
 - Column rendering.

 Usage:
 - Used by the RalphMac app or RalphCore tests through its owning feature surface.

 Invariants/Assumptions:
 - Callers keep usage within the documented responsibilities and owning feature contracts.
 */

import RalphCore
import SwiftUI

@MainActor
extension WorkspaceView {
    @ViewBuilder
    func errorRecoverySheet() -> some View {
        if let error = workspace.diagnosticsState.lastRecoveryError {
            ErrorRecoverySheet(
                error: error,
                workspace: workspace,
                onRetry: { handleRetry(for: error.operation) },
                onDismiss: { workspace.clearErrorRecovery() }
            )
        }
    }

    @ViewBuilder
    func commandPaletteSheet() -> some View {
        CommandPaletteView(
            windowActions: workspaceWindowActions,
            workspaceUIActions: commandActions
        )
        .frame(minWidth: 640, minHeight: 300)
    }

    @ViewBuilder
    func operationalHealthSheet() -> some View {
        OperationalHealthSheet(
            workspaceName: workspace.projectDisplayName,
            summary: workspace.diagnosticsState.operationalSummary,
            issues: workspace.diagnosticsState.operationalIssues,
            watcherHealth: workspace.diagnosticsState.watcherHealth,
            cliHealthStatus: workspace.diagnosticsState.cliHealthStatus,
            onRepair: { handleRepairOperationalHealth() }
        )
    }
}
