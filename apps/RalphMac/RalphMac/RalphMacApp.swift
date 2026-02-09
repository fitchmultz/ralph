/**
 RalphMacApp

 Responsibilities:
 - Define the macOS SwiftUI app entry point.
 - Configure multi-window support with native macOS tab bar integration.
 - Handle window restoration on app relaunch.
 - Provide menu commands for window/tab management and navigation.

 Does not handle:
 - Individual workspace content or CLI operations (see Workspace and WindowView).
 - Sidebar navigation state (see NavigationViewModel).

 Invariants/assumptions callers must respect:
 - The app bundle includes an executable named `ralph` placed alongside the app binary.
 - Window restoration state is stored in UserDefaults.
 - Navigation notifications are sent via NotificationCenter.
 */

public import SwiftUI
import RalphCore

@main
struct RalphMacApp: App {
    @StateObject private var manager = WorkspaceManager.shared
    @Environment(\.scenePhase) private var scenePhase

    var body: some Scene {
        WindowGroup {
            WindowViewContainer()
                .background(
                    VisualEffectView(material: .windowBackground, blendingMode: .behindWindow)
                        .ignoresSafeArea()
                )
        }
        .windowStyle(.hiddenTitleBar)
        .windowToolbarStyle(.unified(showsTitle: false))
        .defaultSize(width: 1400, height: 900)
        .defaultPosition(.center)

        .commands {
            workspaceCommands
            navigationCommands
            taskCommands
        }
    }

    private var workspaceCommands: some Commands {
        CommandMenu("Workspace") {
            Button("New Tab") {
                NotificationCenter.default.post(
                    name: .newWorkspaceTabRequested,
                    object: nil
                )
            }
            .keyboardShortcut("t", modifiers: .command)

            Button("New Window") {
                NotificationCenter.default.post(
                    name: .newWindowRequested,
                    object: nil
                )
            }
            .keyboardShortcut("n", modifiers: [.command, .shift])

            Divider()

            Button("Close Tab") {
                NotificationCenter.default.post(
                    name: .closeActiveTabRequested,
                    object: nil
                )
            }
            .keyboardShortcut("w", modifiers: .command)

            Button("Close Window") {
                NotificationCenter.default.post(
                    name: .closeActiveWindowRequested,
                    object: nil
                )
            }
            .keyboardShortcut("w", modifiers: [.command, .shift])

            Divider()

            Button("Next Tab") {
                NotificationCenter.default.post(
                    name: .selectNextTabRequested,
                    object: nil
                )
            }
            .keyboardShortcut("]", modifiers: [.command, .shift])

            Button("Previous Tab") {
                NotificationCenter.default.post(
                    name: .selectPreviousTabRequested,
                    object: nil
                )
            }
            .keyboardShortcut("[", modifiers: [.command, .shift])

            Divider()

            Button("Duplicate Tab") {
                NotificationCenter.default.post(
                    name: .duplicateActiveTabRequested,
                    object: nil
                )
            }
            .keyboardShortcut("d", modifiers: .command)
        }
    }

    private var navigationCommands: some Commands {
        CommandMenu("Navigation") {
            Button("Show Queue") {
                NotificationCenter.default.post(
                    name: .showSidebarSection,
                    object: SidebarSection.queue
                )
            }
            .keyboardShortcut("1", modifiers: .command)

            Button("Show Quick Actions") {
                NotificationCenter.default.post(
                    name: .showSidebarSection,
                    object: SidebarSection.quickActions
                )
            }
            .keyboardShortcut("2", modifiers: .command)

            Button("Show Run Control") {
                NotificationCenter.default.post(
                    name: .showSidebarSection,
                    object: SidebarSection.runControl
                )
            }
            .keyboardShortcut("3", modifiers: .command)

            Button("Show Advanced Runner") {
                NotificationCenter.default.post(
                    name: .showSidebarSection,
                    object: SidebarSection.advancedRunner
                )
            }
            .keyboardShortcut("4", modifiers: .command)

            Button("Show Analytics") {
                NotificationCenter.default.post(
                    name: .showSidebarSection,
                    object: SidebarSection.analytics
                )
            }
            .keyboardShortcut("5", modifiers: .command)

            Divider()

            Button("Toggle Sidebar") {
                NotificationCenter.default.post(
                    name: .toggleSidebar,
                    object: nil
                )
            }
            .keyboardShortcut("s", modifiers: [.command, .control])

            Divider()

            Button("Toggle View Mode") {
                NotificationCenter.default.post(
                    name: .toggleTaskViewMode,
                    object: nil
                )
            }
            .keyboardShortcut("k", modifiers: [.command, .shift])

            Button("Show Graph View") {
                NotificationCenter.default.post(
                    name: .showGraphView,
                    object: nil
                )
            }
            .keyboardShortcut("g", modifiers: [.command, .shift])
        }
    }

    private var taskCommands: some Commands {
        CommandMenu("Task") {
            Button("New Task...") {
                NotificationCenter.default.post(
                    name: .showTaskCreation,
                    object: nil
                )
            }
            .keyboardShortcut("n", modifiers: [.command, .option])
            
            Divider()
            
            Button("Check for CLI Updates") {
                NotificationCenter.default.post(
                    name: .checkForCLIUpdates,
                    object: nil
                )
            }
        }
    }
}

// MARK: - Window View Container

/// Container view that handles workspace initialization to avoid state mutation during view init.
struct WindowViewContainer: View {
    @StateObject private var manager = WorkspaceManager.shared
    @State private var windowState: WindowState?

    var body: some View {
        Group {
            if let state = windowState {
                WindowView(windowState: state)
            } else {
                ProgressView("Initializing...")
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
            }
        }
        .task {
            // Defer workspace creation to avoid mutating state during view initialization
            if windowState == nil {
                let workspace = manager.createWorkspace()
                windowState = WindowState(workspaceIDs: [workspace.id])
            }
        }
    }
}

// MARK: - Notification Names

extension Notification.Name {
    static let newWorkspaceTabRequested = Notification.Name("newWorkspaceTabRequested")
    static let newWindowRequested = Notification.Name("newWindowRequested")
    static let closeActiveTabRequested = Notification.Name("closeActiveTabRequested")
    static let closeActiveWindowRequested = Notification.Name("closeActiveWindowRequested")
    static let selectNextTabRequested = Notification.Name("selectNextTabRequested")
    static let selectPreviousTabRequested = Notification.Name("selectPreviousTabRequested")
    static let duplicateActiveTabRequested = Notification.Name("duplicateActiveTabRequested")
    static let showTaskCreation = Notification.Name("showTaskCreation")
    static let checkForCLIUpdates = Notification.Name("checkForCLIUpdates")
}
