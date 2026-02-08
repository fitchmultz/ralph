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

struct WindowView: View {
    @State var windowState: WindowState
    @StateObject private var manager = WorkspaceManager.shared

    @Environment(\.openWindow) private var openWindow
    @Environment(\.dismissWindow) private var dismissWindow

    var body: some View {
        TabView(selection: $windowState.selectedTabIndex) {
            ForEach(Array(windowState.workspaceIDs.enumerated()), id: \.element) { index, workspaceID in
                if let workspace = manager.workspaces.first(where: { $0.id == workspaceID }) {
                    WorkspaceView(workspace: workspace)
                        .tabItem {
                            Label(workspace.name, systemImage: "folder")
                        }
                        .tag(index)
                }
            }
        }
        .onChange(of: windowState.workspaceIDs) { _, _ in
            persistState()
        }
        .onChange(of: windowState.selectedTabIndex) { _, _ in
            persistState()
        }
        .onReceive(manager.$workspaces) { _ in
            // Clean up any closed workspaces from this window
            cleanupClosedWorkspaces()
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
    }

    // MARK: - Tab Management

    private func addNewTab() {
        let newWorkspace = manager.createWorkspace()
        windowState.workspaceIDs.append(newWorkspace.id)
        windowState.selectedTabIndex = windowState.workspaceIDs.count - 1
        persistState()
    }

    private func closeActiveTab() {
        guard !windowState.workspaceIDs.isEmpty else { return }

        let index = windowState.selectedTabIndex
        guard index < windowState.workspaceIDs.count else { return }

        let workspaceID = windowState.workspaceIDs[index]

        // Check if workspace has running operations
        if let workspace = manager.workspaces.first(where: { $0.id == workspaceID }) {
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
            let newWorkspace = manager.createWorkspace()
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

    private func cleanupClosedWorkspaces() {
        let validIDs = Set(manager.workspaces.map(\.id))
        let originalCount = windowState.workspaceIDs.count
        windowState.workspaceIDs.removeAll { !validIDs.contains($0) }

        // If we removed workspaces, ensure selection is valid
        if windowState.workspaceIDs.count != originalCount {
            windowState.validateSelection()
            persistState()

            // If no workspaces left, create a new one
            if windowState.workspaceIDs.isEmpty {
                let newWorkspace = manager.createWorkspace()
                windowState.workspaceIDs.append(newWorkspace.id)
                windowState.selectedTabIndex = 0
                persistState()
            }
        }
    }
}
