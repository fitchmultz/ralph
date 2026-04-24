/**
 WorkspaceView+Actions

 Purpose:
 - Execute workspace-scoped retry, repair, navigation, and task-presentation actions.

 Responsibilities:
 - Execute workspace-scoped retry, repair, navigation, and task-presentation actions.
 - Reset navigation-local UI state during repository retargets.

 Does not handle:
 - Sheet composition.
 - Accessibility probe rendering.

 Usage:
 - Used by the RalphMac app or RalphCore tests through its owning feature surface.

 Invariants/Assumptions:
 - Callers keep usage within the documented responsibilities and owning feature contracts.
 */

import RalphCore
import SwiftUI

@MainActor
extension WorkspaceView {
    func handleRetryConnection() {
        Task { @MainActor in
            _ = await workspace.checkHealth()
            if let newStatus = workspace.diagnosticsState.cliHealthStatus, newStatus.isAvailable {
                await workspace.loadTasks()
            }
        }
    }

    func handleRepairOperationalHealth() {
        Task { @MainActor in
            await workspace.repairOperationalHealth()
        }
    }

    func handleRetry(for operation: String) {
        workspace.clearErrorRecovery()

        switch operation {
        case "loadTasks":
            Task { @MainActor in await workspace.loadTasks() }
        case "loadGraphData":
            Task { @MainActor in await workspace.loadGraphData() }
        case "loadCLISpec":
            Task { @MainActor in await workspace.loadCLISpec() }
        case "run", "runVersion", "runInit":
            if workspace.runState.isRunning { workspace.cancel() }
            if navigation.selectedSection == .quickActions {
                workspace.runVersion()
            }
        default:
            Task { @MainActor in await workspace.loadTasks() }
        }
    }

    func handleStartWork() {
        guard let taskID = navigation.selectedTaskID else { return }

        Task { @MainActor in
            do {
                try await workspace.updateTaskStatus(taskID: taskID, to: .doing)
            } catch {
                RalphLogger.shared.error("Failed to start work on task: \(error)", category: .workspace)
            }
        }
    }

    func handleRepositoryRetarget() {
        navigation.resetForRepositoryRetarget()
        showingTaskCreation = false
        showingTaskDecompose = false
    }

    func showTaskCreation() {
        navigation.selectedSection = .queue
        showingTaskCreation = true
    }

    func showTaskDecompose(selectedTaskID: String?) {
        navigation.selectedSection = .queue
        taskDecomposeContext = TaskDecomposeView.PresentationContext(
            selectedTaskID: selectedTaskID ?? navigation.selectedTaskID
        )
        showingTaskDecompose = true
    }

    func showTaskDetail(_ taskID: String) {
        navigation.selectedSection = .queue
        navigation.selectedTaskID = taskID
        navigation.selectedTaskIDs = [taskID]
    }
}
