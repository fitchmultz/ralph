/**
 WindowView

 Responsibilities:
 - Manage the tab-based interface for a single window.
 - Handle workspace tab creation, closure, and switching.
 - Integrate with native macOS tab bar for tear-off support.
 - Persist window state for restoration.

 Does not handle:
 - Workspace content rendering (see WorkspaceView).
 - Cross-window operations.

 Invariants/assumptions callers must respect:
 - windowState is managed by the parent and updated on changes.
 - Uses native SwiftUI TabView with .tabItem for macOS tab bar integration.
 */

import SwiftUI
import RalphCore

@MainActor
struct WindowView: View {
    @State var windowState: WindowState
    @ObservedObject private var manager = WorkspaceManager.shared

    @Environment(\.openWindow) private var openWindow
    @Environment(\.dismissWindow) private var dismissWindow

    var body: some View {
        TabView(selection: $windowState.selectedTabIndex) {
            ForEach(Array(windowState.workspaceIDs.enumerated()), id: \.element) { index, workspaceID in
                if let workspace = manager.workspaces.first(where: { $0.id == workspaceID }) {
                    WorkspaceView(workspace: workspace)
                        .tabItem {
                            Label(workspace.projectDisplayName, systemImage: "folder")
                        }
                        .tag(index)
                }
            }
        }
        .onChange(of: windowState.workspaceIDs) { _, _ in
            validateAndPersistState()
        }
        .onChange(of: windowState.selectedTabIndex) { _, _ in
            persistState()
        }
        .onReceive(manager.$workspaces) { _ in
            // Defer cleanup to avoid state mutation during view update
            Task { @MainActor in
                cleanupClosedWorkspaces()
            }
        }
        .onReceive(NotificationCenter.default.publisher(for: .newWorkspaceTabRequested)) { _ in
            addNewTab()
        }
        .onReceive(NotificationCenter.default.publisher(for: .closeActiveTabRequested)) { _ in
            closeActiveTab()
        }
        .onReceive(NotificationCenter.default.publisher(for: .duplicateActiveTabRequested)) { _ in
            duplicateActiveTab()
        }
        .onReceive(NotificationCenter.default.publisher(for: .selectNextTabRequested)) { _ in
            selectNextTab()
        }
        .onReceive(NotificationCenter.default.publisher(for: .selectPreviousTabRequested)) { _ in
            selectPreviousTab()
        }
        // Handle URL scheme activation for existing workspaces
        .onReceive(NotificationCenter.default.publisher(for: .activateWorkspace)) { notification in
            if let workspaceID = notification.object as? UUID,
               let index = windowState.workspaceIDs.firstIndex(of: workspaceID) {
                windowState.selectedTabIndex = index
                persistState()
            }
        }
        // Handle URL scheme creation of new workspaces
        .onReceive(NotificationCenter.default.publisher(for: .workspaceOpenedFromURL)) { notification in
            if let workspaceID = notification.object as? UUID,
               !windowState.workspaceIDs.contains(workspaceID) {
                windowState.workspaceIDs.append(workspaceID)
                windowState.selectedTabIndex = windowState.workspaceIDs.count - 1
                persistState()
            }
        }
        // Handle app background/termination - force save all state
        .onReceive(NotificationCenter.default.publisher(for: .saveAllWindowStatesRequested)) { _ in
            persistState()
        }
    }

    // MARK: - Tab Management

    private func addNewTab() {
        let preferredDirectory = activeWorkspace()?.workingDirectoryURL ?? manager.workspaces.last?.workingDirectoryURL
        let newWorkspace = manager.createWorkspace(workingDirectory: preferredDirectory)
        windowState.workspaceIDs.append(newWorkspace.id)
        windowState.selectedTabIndex = windowState.workspaceIDs.count - 1
        persistState()
    }

    private func closeActiveTab() {
        guard !windowState.workspaceIDs.isEmpty else { return }

        let index = windowState.selectedTabIndex
        guard index < windowState.workspaceIDs.count else { return }

        let workspaceID = windowState.workspaceIDs[index]
        var fallbackDirectory: URL?

        // Check if workspace has running operations
        if let workspace = manager.workspaces.first(where: { $0.id == workspaceID }) {
            fallbackDirectory = workspace.workingDirectoryURL
            if workspace.isRunning {
                // Show alert before closing - for now, just cancel
                workspace.cancel()
            }
            manager.closeWorkspace(workspace)
        }

        windowState.workspaceIDs.remove(at: index)

        // Adjust selection
        if windowState.workspaceIDs.isEmpty {
            // Create new workspace if none left
            let newWorkspace = manager.createWorkspace(workingDirectory: fallbackDirectory)
            windowState.workspaceIDs.append(newWorkspace.id)
            windowState.selectedTabIndex = 0
        } else {
            windowState.selectedTabIndex = min(index, windowState.workspaceIDs.count - 1)
        }

        persistState()
    }

    private func duplicateActiveTab() {
        guard !windowState.workspaceIDs.isEmpty else { return }

        let index = windowState.selectedTabIndex
        guard index < windowState.workspaceIDs.count else { return }

        let workspaceID = windowState.workspaceIDs[index]
        guard let workspace = manager.workspaces.first(where: { $0.id == workspaceID }) else { return }

        let newWorkspace = manager.duplicateWorkspace(workspace)
        windowState.workspaceIDs.insert(newWorkspace.id, at: index + 1)
        windowState.selectedTabIndex = index + 1
        persistState()
    }

    private func selectNextTab() {
        guard !windowState.workspaceIDs.isEmpty else { return }
        let nextIndex = (windowState.selectedTabIndex + 1) % windowState.workspaceIDs.count
        windowState.selectedTabIndex = nextIndex
        persistState()
    }

    private func selectPreviousTab() {
        guard !windowState.workspaceIDs.isEmpty else { return }
        let prevIndex = windowState.selectedTabIndex == 0
            ? windowState.workspaceIDs.count - 1
            : windowState.selectedTabIndex - 1
        windowState.selectedTabIndex = prevIndex
        persistState()
    }

    // MARK: - State Persistence

    private func persistState() {
        manager.saveWindowState(windowState)
    }

    /// Validates selection bounds and persists state atomically.
    private func validateAndPersistState() {
        var updatedState = windowState
        updatedState.validateSelection()
        windowState = updatedState
        manager.saveWindowState(windowState)
    }

    private func cleanupClosedWorkspaces() {
        let validIDs = Set(manager.workspaces.map(\.id))
        let originalCount = windowState.workspaceIDs.count
        
        // Only modify if there are actually closed workspaces
        let invalidIDs = windowState.workspaceIDs.filter { !validIDs.contains($0) }
        guard !invalidIDs.isEmpty else { return }
        
        windowState.workspaceIDs.removeAll { !validIDs.contains($0) }

        // If we removed workspaces, ensure selection is valid
        if windowState.workspaceIDs.count != originalCount {
            windowState.validateSelection()
            persistState()

            // If no workspaces left, create a new one
            if windowState.workspaceIDs.isEmpty {
                let newWorkspace = manager.createWorkspace(workingDirectory: manager.workspaces.last?.workingDirectoryURL)
                windowState.workspaceIDs.append(newWorkspace.id)
                windowState.selectedTabIndex = 0
                persistState()
            }
        }
    }

    private func activeWorkspace() -> Workspace? {
        guard !windowState.workspaceIDs.isEmpty else { return nil }
        guard windowState.selectedTabIndex < windowState.workspaceIDs.count else { return nil }
        let workspaceID = windowState.workspaceIDs[windowState.selectedTabIndex]
        return manager.workspaces.first(where: { $0.id == workspaceID })
    }
}
