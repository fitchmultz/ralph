/**
 WindowView

 Responsibilities:
 - Manage the tab-based interface for a single window.
 - Handle workspace tab creation, closure, and switching.
 - Expose focused window actions so menu/keyboard commands mutate only the active window.
 - Persist window state for restoration.

 Does not handle:
 - Workspace content rendering (see WorkspaceView).
 - Cross-window command fan-out (handled by focused scene routing).
 - Window tabbing mode configuration (handled by AppDelegate).

 Invariants/assumptions callers must respect:
 - windowState is managed by the parent and updated on changes.
 - Uses native SwiftUI TabView with .tabItem for macOS tab bar integration.
 */

import SwiftUI
import RalphCore

@MainActor
struct WindowView: View {
    // Note: @State must be internal (not private) because WindowViewContainer initializes this view
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
        .overlay(alignment: .topLeading) {
            WindowTabCountAccessibilityProbe(tabCount: windowState.workspaceIDs.count)
        }
        .focusedSceneValue(\.workspaceWindowActions, focusedWindowActions)
        .onChange(of: windowState.workspaceIDs) { _, _ in
            validateAndPersistState()
        }
        .onChange(of: windowState.selectedTabIndex) { _, _ in
            updateFocusedWorkspace()
            persistState()
        }
        .onAppear {
            updateFocusedWorkspace()
        }
        .onReceive(manager.$workspaces) { _ in
            // Defer cleanup to avoid state mutation during view update.
            Task { @MainActor in
                cleanupClosedWorkspaces()
            }
        }
        .modifier(WindowStateNotificationHandlers(
            windowState: $windowState,
            persistState: persistState
        ))
    }

    private var focusedWindowActions: WorkspaceWindowActions {
        WorkspaceWindowActions(
            newWindow: { openWindow(id: "main") },
            newTab: addNewTab,
            closeTab: closeActiveTab,
            closeWindow: closeActiveWindow,
            nextTab: selectNextTab,
            previousTab: selectPreviousTab,
            duplicateTab: duplicateActiveTab
        )
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

        // Check if workspace has running operations.
        if let workspace = manager.workspaces.first(where: { $0.id == workspaceID }) {
            fallbackDirectory = workspace.workingDirectoryURL
            if workspace.isRunning {
                // Show alert before closing - for now, just cancel.
                workspace.cancel()
            }
            manager.closeWorkspace(workspace)
        }

        windowState.workspaceIDs.remove(at: index)

        // Adjust selection.
        if windowState.workspaceIDs.isEmpty {
            // Create new workspace if none left.
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

    private func closeActiveWindow() {
        dismissWindow()
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

        // Only modify if there are actually closed workspaces.
        let invalidIDs = windowState.workspaceIDs.filter { !validIDs.contains($0) }
        guard !invalidIDs.isEmpty else { return }

        windowState.workspaceIDs.removeAll { !validIDs.contains($0) }

        // If we removed workspaces, ensure selection is valid.
        if windowState.workspaceIDs.count != originalCount {
            windowState.validateSelection()
            persistState()

            // If no workspaces left, create a new one.
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
    
    private func updateFocusedWorkspace() {
        manager.focusedWorkspace = activeWorkspace()
    }
}

/// Exposes per-window tab metadata to UI tests without changing visible UI.
private struct WindowTabCountAccessibilityProbe: View {
    let tabCount: Int

    var body: some View {
        Color.clear
            .frame(width: 1, height: 1)
            .allowsHitTesting(false)
            .accessibilityElement(children: .ignore)
            .accessibilityIdentifier("window-tab-count-probe")
            .accessibilityLabel("window-tab-count-\(tabCount)")
            .accessibilityValue("\(tabCount)")
    }
}

// MARK: - State Notifications

/// Notification handlers for global events that are not active-window command dispatch.
@MainActor
struct WindowStateNotificationHandlers: ViewModifier {
    @Binding var windowState: WindowState
    let persistState: () -> Void

    func body(content: Content) -> some View {
        content
            .onReceive(NotificationCenter.default.publisher(for: .activateWorkspace)) { notification in
                if let workspaceID = notification.object as? UUID,
                   let index = windowState.workspaceIDs.firstIndex(of: workspaceID) {
                    windowState.selectedTabIndex = index
                    persistState()
                }
            }
            .onReceive(NotificationCenter.default.publisher(for: .workspaceOpenedFromURL)) { notification in
                if let workspaceID = notification.object as? UUID,
                   !windowState.workspaceIDs.contains(workspaceID) {
                    windowState.workspaceIDs.append(workspaceID)
                    windowState.selectedTabIndex = windowState.workspaceIDs.count - 1
                    persistState()
                }
            }
            .onReceive(NotificationCenter.default.publisher(for: .saveAllWindowStatesRequested)) { _ in
                persistState()
            }
    }
}
