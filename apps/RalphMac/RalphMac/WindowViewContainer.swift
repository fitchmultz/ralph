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
final class HostWindowReference {
    weak var window: NSWindow?
}

@MainActor
struct WindowViewContainer: View {
    private let manager = WorkspaceManager.shared
    @State private var windowState: WindowState?
    @State private var hostWindowReference = HostWindowReference()
    @State private var resolvedHostWindowNumber: Int?
    @State private var initialBootstrapTask: Task<Void, Never>?
    @State private var didRunInitialBootstrap = false
    @State private var didResolveSceneWindowState = false
    @SceneStorage("windowStateID") private var persistedWindowStateID: String = ""
    @Environment(\.openWindow) private var openWindow

    private static let uiTestingWorkspacePathEnvKey = "RALPH_UI_TEST_WORKSPACE_PATH"
    private static let isUITestingLaunch = ProcessInfo.processInfo.arguments.contains("--uitesting")
    private static let minimumWorkspaceWindowSize = NSSize(width: 1200, height: 640)

    var body: some View {
        Group {
            if let state = windowState {
                WindowView(windowState: state, hostWindowReference: hostWindowReference)
                    .background(
                        WorkspaceWindowAnchor(
                            minimumSize: Self.minimumWorkspaceWindowSize,
                            uiTestingEnabled: Self.isUITestingLaunch,
                            onWindowResolved: { resolvedWindow in
                                hostWindowReference.window = resolvedWindow
                                resolvedHostWindowNumber = resolvedWindow.windowNumber
                                if let state = windowState {
                                    WorkspaceWindowRegistry.shared.update(
                                        window: resolvedWindow,
                                        windowStateID: state.id,
                                        workspaceIDs: state.workspaceIDs,
                                        activeWorkspaceID: activeWorkspaceID(for: state)
                                    )
                                } else {
                                    WorkspaceWindowRegistry.shared.register(window: resolvedWindow)
                                }
                            }
                        )
                    )
            } else {
                ProgressView("Initializing...")
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
            }
        }
        .onAppear {
            initializeWindowStateIfNeeded()
            UITestingWindowCoordinator.shared.configureIfNeeded()
            UITestingWindowCoordinator.shared.openAdditionalWindowIfNeeded(openWindow: openWindow)
        }
        .task(id: resolvedHostWindowNumber) { @MainActor in
            guard resolvedHostWindowNumber != nil else { return }
            scheduleInitialBootstrap()
        }
        .onDisappear {
            initialBootstrapTask?.cancel()
            if let window = hostWindowReference.window {
                WorkspaceWindowRegistry.shared.unregister(window: window)
            }
        }
    }

    private func initializeWindowStateIfNeeded() {
        guard !didResolveSceneWindowState else { return }

        if let uiTestingState = uiTestingWindowState() {
            windowState = uiTestingState
            persistedWindowStateID = ""
            didResolveSceneWindowState = true
            return
        }

        let preferredID = UUID(uuidString: persistedWindowStateID) ?? windowState?.id
        let claimedState = manager.claimWindowState(preferredID: preferredID)
        windowState = claimedState
        persistedWindowStateID = claimedState.id.uuidString
        manager.saveWindowState(claimedState)
        didResolveSceneWindowState = true
    }

    private func uiTestingWindowState() -> WindowState? {
        guard ProcessInfo.processInfo.arguments.contains("--uitesting") else { return nil }
        guard let rawPath = ProcessInfo.processInfo.environment[Self.uiTestingWorkspacePathEnvKey],
              !rawPath.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else {
            return nil
        }

        let workspaceURL = Workspace.normalizedWorkingDirectoryURL(
            URL(fileURLWithPath: rawPath, isDirectory: true)
        )
        let workspace = manager.createWorkspace(
            workingDirectory: workspaceURL,
            launchDisposition: .startupPlaceholder
        )
        return WindowState(workspaceIDs: [workspace.id])
    }

    private func performInitialWorkspaceHealthCheck(for state: WindowState) {
        guard let firstWorkspaceID = state.workspaceIDs.first,
              let workspace = manager.workspaces.first(where: { $0.id == firstWorkspaceID }) else {
            return
        }

        workspace.scheduleHealthCheck()
    }

    private func activeWorkspaceID(for state: WindowState) -> UUID? {
        guard !state.workspaceIDs.isEmpty else { return nil }
        guard state.selectedTabIndex < state.workspaceIDs.count else {
            return state.workspaceIDs.first
        }
        return state.workspaceIDs[state.selectedTabIndex]
    }

    private func scheduleInitialBootstrap() {
        guard !didRunInitialBootstrap, let state = windowState else { return }

        didRunInitialBootstrap = true
        initialBootstrapTask?.cancel()
        initialBootstrapTask = Task { @MainActor in
            await Task.yield()
            guard !Task.isCancelled else { return }
            performInitialWorkspaceHealthCheck(for: state)
        }
    }
}
