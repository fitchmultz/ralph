/**
 WorkspaceContractPresentationCoordinator

 Purpose:
 - Capture active workspace presentation diagnostics for noninteractive macOS contract runs.

 Responsibilities:
 - Capture active workspace presentation diagnostics for noninteractive macOS contract runs.
 - Expose machine-readable workspace/window snapshots to in-process contract runners.
 - Track focused/effective workspace presentation state across retarget and scene-routing flows.

 Does not handle:
 - UI-test accessibility probing.
 - Persistent workspace or window restoration.
 - Routing decisions or contract orchestration.

 Usage:
 - Used by the RalphMac app or RalphCore tests through its owning feature surface.

 Invariants/assumptions callers must respect:
 - Normal app behavior must not depend on contract diagnostics being enabled.
 - Snapshots are keyed by workspace ID and resolved against the focused/effective workspace.
 - Visible window counts are derived from live AppKit state at capture time.
 */

import AppKit
import Foundation
import RalphCore

struct WorkspaceContractDiagnosticsSnapshot: Codable, Equatable {
    var workspaceID: String?
    var workspacePath: String?
    var projectDisplayName: String?
    var selectedSection: String?
    var selectedTaskID: String?
    var selectedTaskIDs: [String]
    var showingTaskCreation: Bool
    var showingTaskDecompose: Bool
    var taskDecomposeSelectedTaskID: String?
    var taskCount: Int
    var tasksLoading: Bool
    var tasksErrorMessage: String?
    var isPlaceholder: Bool
    var retargetRevision: UInt64
    var workspaceCount: Int
    var focusedWorkspaceID: String?
    var effectiveWorkspaceID: String?
    var visibleAppWindowCount: Int
    var visibleWorkspaceWindowCount: Int

    static let idle = WorkspaceContractDiagnosticsSnapshot(
        workspaceID: nil,
        workspacePath: nil,
        projectDisplayName: nil,
        selectedSection: nil,
        selectedTaskID: nil,
        selectedTaskIDs: [],
        showingTaskCreation: false,
        showingTaskDecompose: false,
        taskDecomposeSelectedTaskID: nil,
        taskCount: 0,
        tasksLoading: false,
        tasksErrorMessage: nil,
        isPlaceholder: false,
        retargetRevision: 0,
        workspaceCount: 0,
        focusedWorkspaceID: nil,
        effectiveWorkspaceID: nil,
        visibleAppWindowCount: 0,
        visibleWorkspaceWindowCount: 0
    )
}

@MainActor
final class WorkspaceContractPresentationCoordinator: ObservableObject {
    static let shared = WorkspaceContractPresentationCoordinator()

    @Published private(set) var diagnostics = WorkspaceContractDiagnosticsSnapshot.idle

    private var snapshotsByWorkspaceID: [UUID: WorkspaceContractDiagnosticsSnapshot] = [:]
    private let diagnosticsFileURL: URL?

    private init() {
        if let rawPath = ProcessInfo.processInfo.environment["RALPH_WORKSPACE_ROUTING_DIAGNOSTICS_PATH"]?
            .trimmingCharacters(in: .whitespacesAndNewlines),
           !rawPath.isEmpty {
            diagnosticsFileURL = URL(fileURLWithPath: rawPath, isDirectory: false)
        } else {
            diagnosticsFileURL = nil
        }
    }

    func capture(
        workspace: Workspace,
        navigation: NavigationViewModel,
        showingTaskCreation: Bool,
        showingTaskDecompose: Bool,
        taskDecomposeContext: TaskDecomposeView.PresentationContext
    ) {
        let manager = WorkspaceManager.shared
        snapshotsByWorkspaceID[workspace.id] = WorkspaceContractDiagnosticsSnapshot(
            workspaceID: workspace.id.uuidString,
            workspacePath: workspace.identityState.workingDirectoryURL.path,
            projectDisplayName: workspace.projectDisplayName,
            selectedSection: navigation.selectedSection.rawValue,
            selectedTaskID: navigation.selectedTaskID,
            selectedTaskIDs: Array(navigation.selectedTaskIDs).sorted(),
            showingTaskCreation: showingTaskCreation,
            showingTaskDecompose: showingTaskDecompose,
            taskDecomposeSelectedTaskID: taskDecomposeContext.selectedTaskID,
            taskCount: workspace.taskState.tasks.count,
            tasksLoading: workspace.taskState.tasksLoading,
            tasksErrorMessage: workspace.taskState.tasksErrorMessage,
            isPlaceholder: workspace.isURLRoutingPlaceholderWorkspace,
            retargetRevision: workspace.identityState.retargetRevision,
            workspaceCount: manager.workspaces.count,
            focusedWorkspaceID: manager.focusedWorkspace?.id.uuidString,
            effectiveWorkspaceID: manager.effectiveWorkspace?.id.uuidString,
            visibleAppWindowCount: 0,
            visibleWorkspaceWindowCount: 0
        )
        refreshDiagnostics(preferredWorkspaceID: workspace.id)
    }

    func unregister(workspaceID: UUID) {
        snapshotsByWorkspaceID.removeValue(forKey: workspaceID)
        refreshDiagnostics(preferredWorkspaceID: nil)
    }

    func refresh() {
        refreshDiagnostics(preferredWorkspaceID: nil)
    }

    private func refreshDiagnostics(preferredWorkspaceID: UUID?) {
        let manager = WorkspaceManager.shared
        let visibleWindows = NSApp.windows.filter(\.isVisible)
        let visibleWorkspaceWindowCount = visibleWindows.filter(isWorkspaceWindow).count

        let resolvedWorkspaceID = preferredWorkspaceID
            ?? manager.focusedWorkspace?.id
            ?? manager.effectiveWorkspace?.id
            ?? snapshotsByWorkspaceID.keys.sorted { $0.uuidString < $1.uuidString }.first

        var snapshot = resolvedWorkspaceID.flatMap { snapshotsByWorkspaceID[$0] }
            ?? WorkspaceContractDiagnosticsSnapshot.idle
        snapshot.workspaceCount = manager.workspaces.count
        snapshot.focusedWorkspaceID = manager.focusedWorkspace?.id.uuidString
        snapshot.effectiveWorkspaceID = manager.effectiveWorkspace?.id.uuidString
        snapshot.visibleAppWindowCount = visibleWindows.count
        snapshot.visibleWorkspaceWindowCount = visibleWorkspaceWindowCount
        diagnostics = snapshot
        persistDiagnosticsIfNeeded()
    }

    private func isWorkspaceWindow(_ window: NSWindow) -> Bool {
        WorkspaceWindowRegistry.shared.contains(window: window)
            || window.identifier?.rawValue.contains("AppWindow") == true
    }

    private func persistDiagnosticsIfNeeded() {
        guard let diagnosticsFileURL else { return }
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.sortedKeys, .prettyPrinted]
        guard let data = try? encoder.encode(diagnostics) else { return }
        let directory = diagnosticsFileURL.deletingLastPathComponent()
        try? FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        try? data.write(to: diagnosticsFileURL, options: .atomic)
    }
}
