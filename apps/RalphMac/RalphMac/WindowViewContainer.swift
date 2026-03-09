/**
 WindowViewContainer

 Responsibilities:
 - Resolve per-scene window state and bootstrap workspace health for new windows.
 - Coordinate UI-testing window creation and window-count policy through dedicated services.

 Does not handle:
 - The main window layout itself.
 - URL routing.

 Invariants/assumptions callers must respect:
 - Each scene claims at most one persisted `WindowState`.
 - UI-testing workspace bootstrapping is driven by launch arguments and environment.
 */

import SwiftUI
import AppKit
import RalphCore

@MainActor
struct WindowViewContainer: View {
    private let manager = WorkspaceManager.shared
    @State private var windowState: WindowState?
    @State private var didResolveSceneWindowState = false
    @SceneStorage("windowStateID") private var persistedWindowStateID: String = ""
    @Environment(\.openWindow) private var openWindow

    private static let uiTestingWorkspacePathEnvKey = "RALPH_UI_TEST_WORKSPACE_PATH"
    private static let isUITestingLaunch = ProcessInfo.processInfo.arguments.contains("--uitesting")
    private static let minimumWorkspaceWindowSize = NSSize(width: 1200, height: 640)

    var body: some View {
        Group {
            if let state = windowState {
                WindowView(windowState: state)
                    .background(
                        WorkspaceWindowAnchor(
                            minimumSize: Self.minimumWorkspaceWindowSize,
                            uiTestingEnabled: Self.isUITestingLaunch
                        )
                    )
            } else {
                ProgressView("Initializing...")
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
            }
        }
        .task { @MainActor in
            initializeWindowStateIfNeeded()
            UITestingWindowCoordinator.shared.configureIfNeeded()
            UITestingWindowCoordinator.shared.openAdditionalWindowIfNeeded(openWindow: openWindow)
            SettingsService.initialize()
        }
    }

    private func initializeWindowStateIfNeeded() {
        guard !didResolveSceneWindowState else { return }

        if let uiTestingState = uiTestingWindowState() {
            windowState = uiTestingState
            persistedWindowStateID = ""
            didResolveSceneWindowState = true
            performInitialWorkspaceHealthCheck(for: uiTestingState)
            return
        }

        let preferredID = UUID(uuidString: persistedWindowStateID) ?? windowState?.id
        let claimedState = manager.claimWindowState(preferredID: preferredID)
        windowState = claimedState
        persistedWindowStateID = claimedState.id.uuidString
        manager.saveWindowState(claimedState)
        didResolveSceneWindowState = true
        performInitialWorkspaceHealthCheck(for: claimedState)
    }

    private func uiTestingWindowState() -> WindowState? {
        guard ProcessInfo.processInfo.arguments.contains("--uitesting") else { return nil }
        guard let rawPath = ProcessInfo.processInfo.environment[Self.uiTestingWorkspacePathEnvKey],
              !rawPath.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else {
            return nil
        }

        let workspaceURL = URL(fileURLWithPath: rawPath, isDirectory: true)
            .standardizedFileURL
            .resolvingSymlinksInPath()
        let workspace = manager.createWorkspace(workingDirectory: workspaceURL)
        return WindowState(workspaceIDs: [workspace.id])
    }

    private func performInitialWorkspaceHealthCheck(for state: WindowState) {
        guard let firstWorkspaceID = state.workspaceIDs.first,
              let workspace = manager.workspaces.first(where: { $0.id == firstWorkspaceID }) else {
            return
        }

        Task { @MainActor in
            _ = await workspace.checkHealth()
            if workspace.showOfflineBanner {
                workspace.loadCachedTasks()
            }
        }
    }
}
