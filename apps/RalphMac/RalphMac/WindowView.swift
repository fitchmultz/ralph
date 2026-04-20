/**
 WindowView

 Responsibilities:
 - Manage the tab-based interface for a single window.
 - Handle workspace tab creation, closure, and switching.
 - Expose focused window actions so menu/keyboard commands mutate only the active window.
 - Register scene-scoped routing actions so unfocused surfaces can target this window directly.
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
import AppKit
import RalphCore

@MainActor
struct WindowView: View {
    // Note: @State must be internal (not private) because WindowViewContainer initializes this view
    @State var windowState: WindowState
    let hostWindowReference: HostWindowReference
    @ObservedObject private var manager = WorkspaceManager.shared
    @State private var sceneSyncTask: Task<Void, Never>?
    @State private var commandActions = WorkspaceWindowActions()

    @Environment(\.openWindow) private var openWindow
    @Environment(\.dismissWindow) private var dismissWindow

    var body: some View {
        TabView(selection: $windowState.selectedTabIndex) {
            ForEach(Array(windowState.workspaceIDs.enumerated()), id: \.element) { index, workspaceID in
                if let workspace = manager.workspaces.first(where: { $0.id == workspaceID }) {
                    WorkspaceView(workspace: workspace)
                        .tabItem {
                            // Keep the tab compact so the centered label does not sit on the split divider.
                            Label {
                                Text(workspace.projectDisplayName)
                                    .lineLimit(1)
                                    .truncationMode(.middle)
                            } icon: {
                                Image(systemName: "folder")
                            }
                            .labelStyle(.titleAndIcon)
                            .frame(maxWidth: 200)
                        }
                        .tag(index)
                }
            }
        }
        .overlay(alignment: .topLeading) {
            WindowTabCountAccessibilityProbe(tabCount: windowState.workspaceIDs.count)
        }
        .focusedSceneValue(\.workspaceWindowActions, commandActions)
        .onChange(of: windowState.workspaceIDs) { _, _ in
            validateAndPersistState()
            syncWorkspaceWindowRegistry()
            scheduleSceneSync()
        }
        .onChange(of: windowState.selectedTabIndex) { _, _ in
            persistState()
            syncWorkspaceWindowRegistry()
            scheduleSceneSync()
        }
        .onAppear {
            configureCommandActions()
            syncWorkspaceWindowRegistry()
            scheduleSceneSync()
        }
        .onChange(of: manager.workspaces.map(\.id)) { _, _ in
            cleanupClosedWorkspaces()
        }
        .onDisappear {
            sceneSyncTask?.cancel()
            manager.unregisterWindowRouteActions(for: windowState.id)
        }
    }

    private func configureCommandActions() {
        commandActions.configure(
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
        let preferredDirectory = activeWorkspace()?.identityState.workingDirectoryURL
            ?? manager.workspaces.last?.identityState.workingDirectoryURL
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
            fallbackDirectory = workspace.identityState.workingDirectoryURL
            if workspace.runState.isRunning {
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
                let newWorkspace = manager.createWorkspace(
                    workingDirectory: manager.workspaces.last?.identityState.workingDirectoryURL
                )
                windowState.workspaceIDs.append(newWorkspace.id)
                windowState.selectedTabIndex = 0
                persistState()
            }

            syncWorkspaceWindowRegistry()
        }
    }

    private func activeWorkspace() -> Workspace? {
        guard !windowState.workspaceIDs.isEmpty else { return nil }
        guard windowState.selectedTabIndex < windowState.workspaceIDs.count else { return nil }
        let workspaceID = windowState.workspaceIDs[windowState.selectedTabIndex]
        return manager.workspaces.first(where: { $0.id == workspaceID })
    }

    private func syncWorkspaceWindowRegistry() {
        guard let hostWindow = hostWindowReference.window else { return }
        WorkspaceWindowRegistry.shared.update(
            window: hostWindow,
            windowStateID: windowState.id,
            workspaceIDs: windowState.workspaceIDs,
            activeWorkspaceID: activeWorkspace()?.id
        )
    }

    private func updateFocusedWorkspace() {
        manager.markWorkspaceActive(activeWorkspace())
        syncWorkspaceWindowRegistry()
        if RalphAppDefaults.isWorkspaceRoutingContract {
            WorkspaceContractPresentationCoordinator.shared.refresh()
        }
    }

    private func scheduleSceneSync() {
        sceneSyncTask?.cancel()
        sceneSyncTask = Task { @MainActor in
            await Task.yield()
            guard !Task.isCancelled else { return }
            updateFocusedWorkspace()
            registerWindowRouteActions()
        }
    }

    private func registerWindowRouteActions() {
        manager.registerWindowRouteActions(
            for: windowState.id,
            actions: WindowRouteActions(
                containsWorkspace: { workspaceID in
                    windowState.workspaceIDs.contains(workspaceID)
                },
                focusWorkspace: { workspaceID in
                    guard let index = windowState.workspaceIDs.firstIndex(of: workspaceID) else { return }
                    windowState.selectedTabIndex = index
                    updateFocusedWorkspace()
                },
                appendWorkspace: { workspaceID in
                    guard !windowState.workspaceIDs.contains(workspaceID) else { return }
                    windowState.workspaceIDs.append(workspaceID)
                    syncWorkspaceWindowRegistry()
                },
                revealWindow: revealHostWindow,
                persistState: persistState
            )
        )
    }

    private func revealHostWindow() {
        guard let hostWindow = hostWindowReference.window else { return }
        hostWindow.collectionBehavior.insert(.moveToActiveSpace)
        RalphMacPresentationRuntime.reveal(hostWindow)
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
