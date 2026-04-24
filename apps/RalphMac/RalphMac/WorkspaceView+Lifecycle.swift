/**
 WorkspaceView+Lifecycle

 Purpose:
 - Register focused command actions and scene-route handlers for a workspace view.

 Responsibilities:
 - Register focused command actions and scene-route handlers for a workspace view.
 - Refresh deterministic contract diagnostics when scene-visible workspace state changes.

 Does not handle:
 - Section rendering.
 - Error-recovery or task-mutation execution.

 Usage:
 - Used by the RalphMac app or RalphCore tests through its owning feature surface.

 Invariants/Assumptions:
 - Callers keep usage within the documented responsibilities and owning feature contracts.
 */

import RalphCore
import SwiftUI

@MainActor
extension WorkspaceView {
    func configureCommandActions() {
        commandActions.configure(
            showCommandPalette: { showingCommandPalette = true },
            navigateToSection: { section in
                navigation.navigate(to: section)
            },
            toggleSidebar: {
                navigation.toggleSidebar()
            },
            toggleTaskViewMode: {
                navigation.toggleTaskViewMode()
            },
            setTaskViewMode: { mode in
                navigation.setTaskViewMode(mode)
            },
            showTaskCreation: {
                showTaskCreation()
            },
            showTaskDecompose: { taskID in
                showTaskDecompose(selectedTaskID: taskID)
            },
            showTaskDetail: { taskID in
                showTaskDetail(taskID)
            },
            startWorkOnSelectedTask: {
                handleStartWork()
            }
        )
    }

    func registerWorkspaceRouteActions() {
        manager.registerWorkspaceRouteActions(for: workspace.id) { route in
            switch route {
            case .showTaskCreation:
                showTaskCreation()
            case .showTaskDecompose(let taskID):
                showTaskDecompose(selectedTaskID: taskID)
            case .showTaskDetail(let taskID):
                showTaskDetail(taskID)
            }
        }
    }

    func refreshContractDiagnostics() {
        guard RalphAppDefaults.isWorkspaceRoutingContract else { return }
        WorkspaceContractPresentationCoordinator.shared.capture(
            workspace: workspace,
            navigation: navigation,
            showingTaskCreation: showingTaskCreation,
            showingTaskDecompose: showingTaskDecompose,
            taskDecomposeContext: taskDecomposeContext
        )
    }
}
